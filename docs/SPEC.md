# Mkrew — Product & Technical Specification

> Canonical spec. If this file and any other document disagree, this file wins.
> Agents: do NOT read this file top to bottom every session. Read `docs/PROGRESS.md` first,
> then open only the sections your current task needs (search for the `§` anchors).

## Table of contents

- §1 Product overview
- §2 Design principles
- §3 Architecture
- §4 Tech stack & repo layout
- §5 Data model & SQLite schema
- §6 Agent profiles & agent state
- §7 IPC protocol
- §8 Daemon
- §9 CLI
- §10 UI: layout, workspaces & terminals
- §11 UI: per-session status
- §12 UI: Markdown review
- §12A UI: Files (tree and file pane)
- §12B UI: Git (right panel tab and Diff pane)
- §13 UI: keyboard shortcuts
- §14 UI: visual design system
- §15 Non-functional requirements
- §16 Testing strategy & fake agent
- §17 Conventions & Windows notes
- §18 Milestones & acceptance criteria
- §19 Risks, open questions, backlog
- §20 v2: working with agents (M9–M11, M13)

---

## §1 Product overview

**Mkrew** (MK + crew: "MK's crew of agents") is a lightweight, native Rust desktop app for
dispatching tasks to many coding agents (Claude Code, Codex, Oh My Pi, …) across many
projects at once, watching their state, and knowing instantly which agent needs the user.
Reading and editing code is delegated to the Zed editor; Mkrew only orchestrates.

**Target user:** one developer running 2–5 projects in parallel, 1–2 agents each, on a
mid-range machine. Windows native is first-class; Linux and macOS are supported.

**Goals**

- Lightweight: UI idle < 150 MB RAM, daemon < 50 MB (agent processes excluded).
- Agents live independently of the window: closing or crashing the UI never stops an agent.
- The user learns within 2 s that an agent needs input or has finished.
- New agents are added with one TOML file, no code changes.
- Task board: every task belongs to a workspace and an agent and runs with one action.
- No forced git worktrees: agents run directly in the project folder by default.
- Each workspace opens many terminals: tabs, each tab split into panes, each pane an agent or a shell.
- Every session shows its own agent status on its pane, its tab and the sidebar.
- Fully keyboard-driven; every shortcut is configurable.
- Review Markdown files (plans, specs) in-app: preview, comment on passages, send comments
  back to the very agent session that wrote the plan so it updates the file in the same context.
- Browse project files, view and edit them (a plain editor with find & replace, ADR 0028); see git changes, stage, commit, and compare
  against a base branch (default `main`, configurable per workspace) like Orca; comment on diff
  lines and send the comments to an agent.
- Simple, calm, beautiful UI — soft, not rigid.
- v2 (§20, M9–M11, M13): make the daily loop with agents faster — notice who needs you, answer
  without switching, instruct precisely, review and undo per turn, and isolate parallel work in
  worktrees when asked.

**Non-goals**

- Not an IDE: no LSP, completion or multi-cursor. A file pane has a plain editor (ADR 0028); diffs are read-only; bigger edits happen in Zed.
- No native mobile app, cloud sync or user accounts. A browser on the user's own tailnet can watch and answer sessions through `mk web` (ADR 0052). Optional phone notifications (§20.0 `[web] push`, ADR 0067) are the other exception to "no cloud" and are described below.
- No GitHub/Linear integration in v1.
- Mkrew never calls LLM APIs itself; it only runs CLI agents the user installed. One exception:
  voice input (§20G) sends a recording the user started to the speech-to-text API they configured
  (ADR 0045).
- Mkrew runs no cloud service of its own. Its only outbound requests are voice input (above) and,
  only while the user has turned `[web] push` on and a phone has opted in, end-to-end encrypted
  Web Push messages to that phone's push service (Apple, Google, Mozilla or Microsoft; ADR 0067).

**Naming**

| Thing | Name |
| --- | --- |
| App (UI binary) | `mkrew` |
| Daemon binary | `mkrew-daemon` |
| CLI binary | `mk` |
| Crates | `mkrew-core`, `mkrew-protocol`, `mkrew-store`, `mkrew-daemon`, `mkrew-cli`, `mkrew-ui`, `mkrew-fakeagent` |
| Env vars | `MKREW_SESSION_ID`, `MKREW_SOCKET`, `MKREW_MK`, `MKREW_LOG` |
| Config dir | `<config>/mkrew/` (e.g. `~/.config/mkrew/`, `%APPDATA%\mkrew\`) |
| Data dir | `<data>/mkrew/` with `mkrew.db`, `logs/`, `sessions/`, `backups/` |
| IPC endpoint | Unix socket `$XDG_RUNTIME_DIR/mkrew.sock` (fallback `<data>/mkrew.sock`); Windows named pipe `\\.\pipe\mkrew-<username>` |

---

## §2 Design principles

When the spec is silent, decide by these principles, in priority order.

1. **An agent is just a terminal command.** No per-agent code in the core. Every agent
   difference lives in its TOML profile.
2. **The daemon is the single source of truth.** It owns tasks, sessions, PTYs. UI and CLI are
   clients that may come and go at any time.
3. **Simple first, optimize later.** Shell out to `git` instead of a git library; JSON over a
   socket instead of a binary protocol; SQLite instead of anything fancier.
4. **Windows is first-class.** Every feature must work on native Windows (ConPTY, named pipes).
5. **Never lose user data.** Never delete files, never auto-commit in the user's projects, never
   run branch-changing git commands on its own. Git actions (stage, commit, push) happen only when
   the user triggers them; destructive actions (discard) always require confirmation; never force-push.
   Read-only snapshots (`git write-tree` with a temporary index, §20C) and task worktrees on their
   own `mkrew/…` branch (§20E) are allowed: neither changes the user's branch, HEAD, index or files
   (ADR 0010).
6. **Few dependencies.** Every new crate needs a reason; prefer popular, maintained crates.

---

## §3 Architecture

Three binaries talk over a local socket. The daemon holds all state and agent processes; UI and
CLI are clients.

```
┌───────────────┐   local socket (JSON)   ┌─────────────────────────────┐
│  mkrew (UI)   │ ◄─────────────────────► │  mkrew-daemon               │
│  GPUI         │   request / event       │  - PTY of every session     │
│  board, inbox │                         │  - SQLite                   │
│  terminals    │                         │  - file watcher             │
│  docs review  │                         │  - OS notifications         │
└──────┬────────┘                         └──────────────▲──────────────┘
       │ spawns `zed path[:line]`                        │ local socket
       ▼                                                 │
┌───────────────┐                         ┌──────────────┴──────────────┐
│  Zed          │                         │  mk (CLI)                   │
│  read / edit  │                         │  - typed by the user        │
└───────────────┘                         │  - called by agent hooks    │
                                          └─────────────────────────────┘
```

**Another computer (ADR 0089):** a machine window (`mkrew-ui --machine <name>`) runs `ssh <host> mk proxy` and speaks §7 over its pipes; the other computer's daemon keeps its local owner-only socket. This computer's window spawns one machine window per entry of `<config>/mkrew/machines.toml` at startup. Quit, closing this computer's window (without a tray), the update install and Remove send the machine window a `quit` line on its stdin: it runs its own quit path (asking about unsaved Files changes) and exits by itself. A machine window keeps its Inbox "viewed" state in its own file `ui_viewed-<key>.json` in the data dir.

**Lifecycle**

- UI starts → connects to the daemon; if none is running it spawns `mkrew-daemon` detached,
  then reconnects.
- Connection lost or a connect fails → the UI shows why with Reconnect / Retry and also retries
  on its own after 1 s, 2 s, 5 s, 10 s, then every 30 s (each try may start the daemon, like
  startup; one try at a time; the status line says "Reconnecting automatically in N s…"; a timer
  per try, no per-frame work). Reconnect / Retry tries at once and restarts the backoff. The task
  dialog and the Add workspace / clone dialog stay open with their text (they cannot send until
  connected); everything mirroring daemon state is cleared. With no workspace the main area offers
  Add workspace (Ctrl+Shift+K).
- UI closes → daemon and all agents keep running.
- `mk daemon stop` or a reboot → agents stop. On next start the daemon restores workspaces,
  tasks, tabs and sessions; agents whose profile has `resume` and that reported their own
  session id are relaunched into their old conversation: task sessions (running/needs_input),
  and agent panes without a task that a tab still showed.
- Exactly one daemon per user, enforced by a lock file in the data dir.

**Agent state flow:** agent (e.g. Claude Code) fires a hook → the hook runs `mk hook <agent> <event>`
→ the CLI sends `hook.report` to the daemon → the daemon updates the session, moves the task,
pushes events to UIs and fires an OS notification. Agents without hooks get state from screen
detection.

---

## §4 Tech stack & repo layout

Rust stable, one Cargo workspace. When adding a crate, check its latest version and current API
(docs.rs / its repo) — never rely on memory.

| Concern | Crate / tool | Notes |
| --- | --- | --- |
| Async runtime | `tokio` | daemon, CLI |
| UI | `gpui` + `gpui-component` | crates.io `gpui-pre` (imported as `gpui`) + `gpui-kit` (gpui-component, `longbridge/gpui-kit`), pinned to the gpui version gpui-kit needs (ADR 0001). Fallback: Tauri + React + xterm.js (see §19, decided in M0) |
| PTY | `portable-pty` | ConPTY on Windows |
| Terminal emulation | `alacritty_terminal` | UI draws the cell grid itself |
| IPC | `interprocess` | named pipe / Unix socket |
| Serialization | `serde`, `serde_json`, `toml` | |
| Database | `rusqlite` (feature `bundled`) | hand-written numbered migrations |
| IDs | `ulid` | time-sortable |
| CLI | `clap` (derive) | |
| File watching | `notify` | conflicts (M5), docs (M6) |
| OS notifications | `notify-rust` (Linux, macOS); WinRT toasts through `windows` (Windows, ADR 0079) | |
| Paths | `directories`, `dunce` | |
| Markdown | `pulldown-cmark` | docs review (M6) |
| Syntax highlighting | `syntect` (pure-Rust regex feature, lazy-loaded) | files & diffs (M7) |
| Diagrams | `mermaid-rs-renderer` (no default features) | Mermaid in the Docs preview (ADR 0021) |
| File walking | `ignore` | respects .gitignore (M7) |
| Fuzzy matching | `nucleo-matcher` | palette Files scope (M7) |
| Content search | `grep-searcher`, `grep-regex`, `grep-matcher` | `fs.search` (ripgrep's engine over the files `ignore` walks) |
| Diff | `similar` | |
| Microphone | `cpal` | voice input (§20G); WASAPI on Windows, ALSA on Linux |
| HTTPS client | `ureq` (rustls) | voice input speech to text (§20G); Jira REST (§20I) |
| Base64 | `base64` | Jira Basic auth header (§20I) |
| Errors | `thiserror` (libs), `anyhow` (bins) | |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | |
| Tests | `insta`, `tempfile` | |

**Repo layout**

```
mkrew/
├── Cargo.toml                # workspace
├── AGENTS.md / CLAUDE.md     # agent instructions
├── docs/                     # SPEC, PROGRESS, BUILD_LOOP, HUMAN_CHECKS, decisions/, screenshots/
├── scripts/                  # verify, autobuild, status, package (setup .exe / tar.gz / macOS app), fetch-conpty (Windows ConPTY files, §17)
├── packaging/                # mkrew.iss (Inno Setup 6 setup .exe, installs conpty.dll and OpenConsole.exe); macos/ (app icon source, entitlements)
├── agents/                   # sample profiles: claude.toml codex.toml omp.toml shell.toml fake.toml
└── crates/
    ├── mkrew-core/           # models, config, profile parsing, state machine. NO I/O beyond files, except the `tailscale` CLI runs of `mkrew_core::tailscale` (ADR 0053).
    ├── mkrew-protocol/       # Request/Response/Event types, frame codec, snapshot → ANSI
    ├── mkrew-store/          # SQLite + migrations
    ├── mkrew-daemon/         # bin: PTYs, sessions, detectors, watchers, notifications
    ├── mkrew-cli/            # bin `mk`
    ├── mkrew-ui/             # bin `mkrew` (GPUI)
    └── mkrew-fakeagent/      # bin used by tests and demos (§16)
```

`mkrew-core` and `mkrew-protocol` must not depend on tokio, GPUI or SQLite.

---

## §5 Data model & SQLite schema

Project (= workspace) contains Tasks; a Task spawns Sessions; a Session runs an AgentProfile.
Profiles come from TOML files and are not stored in the DB. IDs are ULIDs; times are unix ms.

```sql
-- migrations/0001_init.sql (M1)
CREATE TABLE project (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  path        TEXT NOT NULL UNIQUE,      -- absolute, normalized
  dev_command TEXT,
  default_agent TEXT,                    -- profile id used for new tasks/panes
  review_globs TEXT,                     -- JSON array, NULL = defaults (§12)
  created_at  INTEGER NOT NULL,
  archived    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE task (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES project(id),
  title         TEXT NOT NULL,
  prompt        TEXT NOT NULL DEFAULT '',
  agent_profile TEXT NOT NULL,
  status        TEXT NOT NULL,           -- todo|running|needs_input|review|done
  position      REAL NOT NULL,
  checkpoint    TEXT,                    -- git HEAD before run (M5)
  checkpoint_at INTEGER,                 -- Unix ms the checkpoint was taken (undo, ADR 0005)
  error_flag    INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE session (
  id                TEXT PRIMARY KEY,
  task_id           TEXT REFERENCES task(id),     -- NULL = free terminal
  project_id        TEXT NOT NULL REFERENCES project(id),
  agent_profile     TEXT NOT NULL,
  kind              TEXT NOT NULL DEFAULT 'agent', -- agent|shell
  title             TEXT,                -- user's pane name (session.rename); the agent's terminal title is memory-only (ADR 0031)
  color             TEXT,                -- user's pane color: a workspace color name (session.set_color, ADR 0037)
  native_session_id TEXT,
  state             TEXT NOT NULL,       -- starting|working|needs_input|idle|done|exited|unknown
  exit_code         INTEGER,
  cols              INTEGER NOT NULL,
  rows              INTEGER NOT NULL,
  started_at        INTEGER NOT NULL,
  ended_at          INTEGER
);

CREATE TABLE session_event (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES session(id),
  kind       TEXT NOT NULL,              -- state_changed|hook|files_changed|...; files_changed/hook/edited rows are pruned after 7 days (ADR 0060)
  payload    TEXT NOT NULL,              -- JSON
  at         INTEGER NOT NULL
);

-- migrations/0002_terminals.sql (M2)
CREATE TABLE terminal_tab (
  id         TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id),
  title      TEXT NOT NULL,
  position   REAL NOT NULL,
  layout     TEXT NOT NULL               -- JSON split tree; leaves are sessions, files or the diff (ADR 0108)
);

-- migrations/0003_session_state.sql (M3, ADR 0003)
ALTER TABLE session ADD COLUMN state_source TEXT NOT NULL DEFAULT 'daemon'; -- hook|screen|daemon
ALTER TABLE session ADD COLUMN state_changed_at INTEGER NOT NULL DEFAULT 0;
UPDATE session SET state_changed_at = COALESCE(ended_at, started_at);
CREATE INDEX session_event_session ON session_event(session_id, id);

-- migrations/0005_doc_review.sql (M6; 0004 = checkpoint_at)
CREATE TABLE doc_comment (
  id          TEXT PRIMARY KEY,          -- 'c_' + ULID (shown to agents, §12)
  project_id  TEXT NOT NULL REFERENCES project(id),
  file_path   TEXT NOT NULL,             -- relative to project root
  parent_id   TEXT REFERENCES doc_comment(id), -- thread root of a reply; replies keep status 'open'
  author      TEXT NOT NULL,             -- 'user' or agent profile id
  body        TEXT NOT NULL,
  start_line  INTEGER,
  end_line    INTEGER,
  quote       TEXT,
  status      TEXT NOT NULL,             -- open|sent|addressed|resolved|outdated
  sent_to     TEXT REFERENCES session(id),
  sent_snapshot TEXT,                    -- file content when sent, for diff
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
CREATE INDEX doc_comment_file ON doc_comment(project_id, file_path);

-- migrations/0006_files_git.sql (M7)
ALTER TABLE project ADD COLUMN base_ref TEXT;                          -- NULL = auto-detect
ALTER TABLE doc_comment ADD COLUMN context TEXT NOT NULL DEFAULT 'doc'; -- doc|diff

-- migrations/0007_dev_server.sql (M8)
ALTER TABLE project ADD COLUMN dev_autostart INTEGER NOT NULL DEFAULT 0; -- start dev_command after daemon start

-- migrations/0011_run_commands.sql (Run button, ADR 0018)
CREATE TABLE run_command (
  project_id TEXT NOT NULL REFERENCES project(id),
  position   INTEGER NOT NULL,           -- order in the menu and in the run tab
  name       TEXT NOT NULL,              -- unique per project, e.g. "BE"
  dir        TEXT NOT NULL DEFAULT '.',  -- relative to the project folder
  command    TEXT NOT NULL,              -- typed into the shell, e.g. "npm run dev"
  env_file   TEXT,                       -- relative to the project folder (ADR 0049), e.g. "EA-BE/.env"
  env        TEXT NOT NULL DEFAULT '[]', -- JSON [[key, value], …], applied after env_file
  PRIMARY KEY (project_id, name)
);
INSERT INTO run_command (project_id, position, name, command)
  SELECT id, 0, 'dev', dev_command FROM project WHERE dev_command IS NOT NULL;
ALTER TABLE project DROP COLUMN dev_command;
ALTER TABLE project RENAME COLUMN dev_autostart TO run_autostart;
```

v2 migrations 0008 (M11) and 0009 (M13) are listed in §20; `0010_session_interrupted.sql` adds
`session.interrupted INTEGER NOT NULL DEFAULT 0` (ADR 0017). `0012_session_hidden.sql` adds
`session.hidden INTEGER NOT NULL DEFAULT 0` (ADR 0019). `0013_project_color.sql` adds
`project.color TEXT` (ADR 0020). `0014_project_default_shell.sql` adds
`project.default_shell TEXT` (shell profile for new shell panes, NULL = not picked; ADR 0032).
`0015_session_color.sql` adds `session.color TEXT` (ADR 0037). `0016_pinned.sql` adds `pinned INTEGER NOT NULL DEFAULT 0` to `project`, `session` and
`terminal_tab` (pinned workspaces, sessions and tabs lead their lists, §10).
`0017_session_plan.sql` adds `session.plan_path TEXT` (the linked plan `.md`: relative to the project folder with `/` when inside, else absolute; NULL = none; ADR 0039).
`0021_project_jira_keys.sql` adds `project.jira_keys TEXT` (JSON array of Jira project keys, NULL = none; §20I, ADR 0066).
`0022_viewed_files.sql` adds table `viewed_file(project_id, worktree_task_id, repo, list, path, hash, marked_at)`, one row per file the user marked as viewed in the Git tab (§12B, ADR 0065).

Split-tree example (`terminal_tab.layout`): vertical split, right half split horizontally.
`vertical` = children side by side (first left), `horizontal` = stacked (first on top); `ratio`
is the first child's share; every split has exactly two children; `null` = a tab without panes.
Leaves are sessions, files, the diff or web pages (ADR 0108, 0111): `{"session": "<ulid>", "chat": true}` (`chat` optional), `{"file": "src/a.rs", "task": "<ulid>", "md": "split", "preview": true}` (`task`, `md` and `preview` optional; the path is relative to the folder the file is of, with `/`, or absolute for a note in an absolute notes folder; `task` = the task worktree the file is of, absent = the project folder, which is what a layout of protocol 58 or 60 means), `{"diff": true}` and `{"web": "https://example.com/", "id": "<ulid>"}` (a browser pane, ADR 0111; `""` before its first page; `id` is required, so two panes can show the same URL).
A key (a session, a file's path with its `task`, the diff, a web page's `id`) appears at most once in a tab, and a file or the diff at most once per workspace; the same path in the project folder and in a worktree are two files.
The daemon reads only the leaves with a `session` key (`cut`, `layout_sessions`) and keeps the others as they are; a leaf the UI cannot parse is dropped with a warning and the rest of the tab loads.

```json
{"split":"vertical","ratio":0.5,"children":[
  {"session":"01JA..."},
  {"split":"horizontal","ratio":0.6,"children":[{"session":"01JB..."},{"session":"01JC..."}]}
]}
```

**Task status transitions** (automatic; the user can still drag cards):

| Event | New task status |
| --- | --- |
| Run pressed, session starts | running |
| Session → needs_input | needs_input |
| Session needs_input → working | running |
| Session done, or exited with code 0 | review |
| Session exited non-zero | needs_input + `error_flag = 1` |
| User approves | done |

SQLite runs in WAL mode. Schema changes are always a new migration file; never edit old ones.

---

## §6 Agent profiles & agent state

Each agent is a TOML file in `<config>/mkrew/agents/`; the file stem is the profile id.
Sample profiles live in the repo's `agents/` and are copied to the config dir on first run if absent.

```toml
# agents/claude.toml
name    = "Claude Code"
# Short badge before the agent name (≤ 3 characters). Missing = the id's first letter.
icon    = "✻"
command = "claude"
args    = []
# How the task prompt reaches the agent:
#   "arg"  = append prompt as the last argument
#   "type" = type prompt into the PTY once the agent is ready, then Enter
#   "none" = do not send
prompt_mode = "arg"
# Optional extra environment variables for this agent
env = {}
# Resume command; {session_id} = native_session_id. Empty = no resume. Split on whitespace,
# "…" / '…' group words (no escapes); {session_id} is replaced inside each word.
resume = "claude --resume {session_id}"
# One-shot text (✦ commit message §12B, ✦ improve prompt §20B.1): non-interactive command,
# split like `resume`; the prompt goes on stdin, stdout is the answer. Empty = not offered.
oneshot = "claude -p"
# Ask AI (§20H, ADR 0061): any question, outside every project; split like `oneshot`, the
# question on stdin. `ask` missing = `oneshot`; `ask_web` missing = no "Web search".
ask     = 'claude -p --safe-mode --tools "" --no-session-persistence --system-prompt "…"'
ask_web = 'claude -p --safe-mode --tools WebSearch --no-session-persistence --system-prompt "…"'

[state]
mode = "hook+screen"            # "hook" | "screen" | "hook+screen"
idle_after_ms = 3000
needs_input_patterns = ['(?i)do you want to', '(?i)\(y/n\)', '❯ 1\. Yes']
```

```toml
# agents/codex.toml
name = "Codex"
icon = "⌬"
command = "codex"
prompt_mode = "arg"
resume = "codex resume {session_id}"
oneshot = "codex exec --sandbox read-only --skip-git-repo-check -"   # prompt on stdin ("-")
[state]
mode = "hook+screen"
idle_after_ms = 3000
needs_input_patterns = ['(?i)allow command', '(?i)approve']
```

```toml
# agents/omp.toml
name = "Oh My Pi"
icon = "π"
command = "omp"
prompt_mode = "type"
resume = "omp --resume {session_id}"   # {session_id} reported by the Mkrew extension
oneshot = "omp -p --no-session"  # one-shot text; prompt on stdin
# tsp = true                      # native surface (ADR 0107): on by default for omp_jsonl profiles; false = omp draws as text
[state]
mode = "hook+screen"
idle_after_ms = 3000
needs_input_patterns = []
```

```toml
# agents/copilot.toml
name = "GitHub Copilot"
icon = "⊙"
command = "copilot"
prompt_mode = "type"             # copilot takes no positional prompt
resume = "copilot --resume={session_id}"
# no oneshot: `copilot -p` takes the prompt as an argument, not on stdin
[state]
mode = "hook+screen"
idle_after_ms = 3000
needs_input_patterns = []
```

`agents/shell.toml` (kind shell, no command = the OS default shell), the shell profiles
`pwsh.toml`, `powershell.toml`, `cmd.toml`, `bash.toml` (first `bash` on PATH) and `git-bash.toml`
(`C:\Program Files\Git\bin\bash.exe`) (ADR 0032), and `agents/fake.toml` (command =
`mkrew-fakeagent`, see §16) also ship. Seeding writes every missing sample at each daemon start and
never overwrites one. Optional key `kind = "agent" | "shell"` (default `agent`); a shell profile
may omit `command` to use the OS default shell (`%COMSPEC%` / `$SHELL`). Optional key `icon` (≤ 3 characters, ADR 0025) is the badge shown
before the agent name; a missing or blank one uses the shipped sample's icon for the same id,
else the id's first letter, uppercased (the shipped `claude`, `codex`, `copilot` and `omp` ids
show their logo instead, ADR 0058 — the one per-agent exception to §2.1, display only); a longer
one is a parse error. Unknown keys and invalid `needs_input_patterns` regexes are parse errors.
Profiles written before `oneshot` existed are not updated (seeding never overwrites); ✦ then
answers `invalid_state` naming the key and the file.

Optional key `tsp` (bool, ADR 0107) turns on omp's native surface.
Absent: on when the profile's `[usage] format` is `omp_jsonl`, off otherwise, so an existing omp profile gets it without an edit.
`tsp = false` turns it off; `tsp = true` on an agent that does not speak TSP (no hello ever comes): the pane shows Starting for 10 s after the first output, then the terminal grid with one "omp is drawing as text" toast, and Chat is unavailable for it meanwhile (`chat::available` needs `tsp` `Off`).
With it on, the daemon starts the agent with `PI_TUI_NATIVE=1`, answers its hello and keeps the surface (§11); every other PTY the daemon opens gets `PI_TUI_NATIVE=0`, so an omp started by hand in a shell pane draws text rows.
The value is read when a session starts.

Two optional keys serve the `/` picker (§20B.7, ADR 0094):

```toml
cli = "claude"                  # "claude" | "codex" | "omp" | "copilot"
[slash]
hide = ["desktop", "upgrade"]   # names removed from the picker
[[slash.add]]                   # a command the scan cannot see; wins over every other source
name = "deploy"
description = "Deploy to staging"
argument_hint = "<env>"         # optional
```

`cli` names the CLI the agent runs.
A missing `cli` is inferred from the file stem of `command`, lowercased, with `.exe`, `.cmd`, `.bat` and `.ps1` removed: `claude`, `codex`, `omp` or `copilot`.
A wrapper (`npx`, a script) infers nothing, so the user sets `cli`.
A shell profile (`kind = "shell"`) never has a CLI.
An unknown `cli` value is a parse error, listed like the other profile errors.
The shipped `agents/*.toml` state `cli` explicitly; `AgentProfileInfo` (§7) does not change.
`[slash] hide` removes rows by name after every source is merged.
`[[slash.add]]` rows get the source `profile`.

Patterns above are starting points only. They must be refined by recording real agent output
(add the recordings as test fixtures under `crates/mkrew-core/tests/fixtures/`).

**Session states**

| State | Meaning | Source |
| --- | --- | --- |
| starting | spawned, no output yet | daemon |
| working | the visible screen text changed in the last `idle_after_ms`, or hook says busy | hook / screen |
| needs_input | waiting for the user to answer/approve | hook / pattern |
| idle | the visible screen text unchanged for `idle_after_ms` (cursor, title, color or identical-redraw output does not count), no pattern match | screen |
| done | hook says the turn finished | hook |
| exited | process exited | daemon |
| unknown | cannot tell | default |

Hook events take priority; screen detection applies only when the mode includes `screen` and no
hook event arrived in the last 10 s. Patterns match the last lines of the **emulated screen**
(the daemon's per-session terminal emulator, §8), never the raw byte stream: TUIs redraw with
cursor movement, so stripping ANSI from the raw stream gives garbage.

The daemon scans a session's screen at most 4 times a second (a scan after the last output closes the gap, so the final screen is always classified) and the state machine reads only the last 4 non-blank rows up to the cursor; a state shown by the screen can therefore appear up to 250 ms after the output (ADR 0088).
A session with a queued `session.send`, and a run session, is scanned on every output, and while the machine ignores the screen (hook-only profile, or a hook event in the last 10 s) no scan work is done.
A session whose `tsp` is `live` (§11, ADR 0107) is hook-only while its surface is open: the screen text stays still, so the machine ignores the screen and the state comes from the hooks (`unknown` without them); when the surface closes or the session falls back, screen detection resumes.

**Hook integrations — `mk integration install|uninstall|status <agent>`**

- The daemon sets `MKREW_SESSION_ID`, `MKREW_SOCKET` and `MKREW_MK` (the absolute path of the
  `mk` next to the daemon; the bare `mk` (`mk.exe` on Windows) when none sits next to it, and then
  `mk hook` does not forward) in every PTY it creates (ADR 0059).
- Hook configs never run a bare `mk`: `mk integration install` writes the canonical absolute path
  of the `mk` that runs it; `mk hook` inside a pane re-runs itself through `MKREW_MK` when that is
  another file, so the daemon's own version always reports. Reinstall after moving Mkrew.
- Claude Code: hooks in `~/.claude/settings.json` for `SessionStart`, `UserPromptSubmit`,
  `Notification`, `Stop`, each in exec form `{"type": "command", "command": "<mk>", "args":
  ["hook", "claude", "<event>"]}` (no shell: Git Bash and PowerShell expand variables
  differently). The hook reads JSON from stdin and takes `session_id` as `native_session_id`.
- Codex: `notify = ["<mk>", "hook", "codex"]` in `~/.codex/config.toml` (argv, no shell); Codex
  passes JSON as an argument.
- Oh My Pi: `mk integration install omp` writes the Mkrew-owned extension
  `<PI_CODING_AGENT_DIR or ~/.omp/agent>/extensions/mkrew.ts`; it runs `MKREW_MK` (falling back to
  `mk`) as `hook omp <busy|needs_input|done|session_start> {"session_id": …}` (root session only:
  not with `OMPCODE=1`, not without a UI).
- GitHub Copilot CLI: `mk integration install copilot` writes the Mkrew-owned
  `<COPILOT_HOME or ~/.copilot>/hooks/mkrew.json` (`sessionStart`, `userPromptSubmitted`,
  `agentStop`, `notification` matching `permission_prompt|elicitation_dialog`), each
  `exec: <mk>`, `args: [hook, copilot, <event>]`, JSON on stdin, `sessionId` = `native_session_id`.
- Install treats an older Mkrew entry (bare `mk` or any `mk` path) as its own: status says
  `partly installed`, install replaces it, uninstall removes it.
- A failed `mk hook` (no daemon, `version_mismatch`, timeout) exits 1 and appends one line to
  `<data>/logs/hook.log` (rotated to `hook.log.1` above 1 MiB): agents drop hook stderr. The
  daemon logs a refused `hello` at `warn`.
- Owned files (omp, Copilot): install writes the whole file (backing up a different one),
  uninstall deletes it. Install prints the `resume` line when the user's profile lacks one.
- Install must: back up the original file (timestamped `.bak`), merge instead of overwrite, be
  idempotent, and have a working uninstall.
- Before coding this, check the CURRENT hook formats in the official Claude Code and Codex docs.
- `mk hook` without `MKREW_SESSION_ID` exits 0 silently (agent running outside Mkrew).
- Event mapping (`mkrew_core::hook`): Claude `UserPromptSubmit` → working, `Notification` with
  `notification_type` `permission_prompt`/`elicitation_*`/`agent_needs_input` (or none) →
  needs_input, `Stop` → done, `SessionStart` and other types → recorded only; Codex
  `agent-turn-complete` → done, `thread-id` = `native_session_id`; fake `busy`/`needs_input`/`done`;
  omp `busy`/`needs_input`/`done` like fake; Copilot `userPromptSubmitted` → working,
  `notification` `permission_prompt`/`elicitation_dialog` → needs_input, `agentStop` → done.

---

## §7 IPC protocol

Length-prefixed JSON frames (u32 big-endian length). Three frame types: `request` (client → daemon,
has `id`), `response` (same `id`), `event` (daemon push, no `id`). All types live in `mkrew-protocol`.

```json
{"type":"request","id":7,"method":"task.run","params":{"task_id":"01J...","cols":120,"rows":40}}
{"type":"response","id":7,"result":{"session_id":"01J..."}}
{"type":"response","id":8,"error":{"code":"not_found","message":"task 01J... not found"}}
{"type":"event","event":"session.state","data":{"session_id":"01J...","state":"needs_input"}}
```

**Requests**

| Method | Main params | Returns |
| --- | --- | --- |
| `hello` | `client` (ui/cli/hook), `protocol_version` | `daemon_version` |
| `project.list/add/update/archive` | `path`, `name`, `run_commands`, `run_autostart`, `color`, `pinned` (update only), `position` (update only), `review_globs`, `default_agent?`, `default_shell?`, `jira_keys?` | Project (`update`: absent field = unchanged, `null` = cleared; `pinned` is a bool; `position` is an integer, the sidebar order inside the pinned or unpinned group (ascending, 0 = never moved, then by age); `run_commands` replaces the list; each command is trimmed and checked (name 1–20 chars without `/` `:`, unique; one-line command; relative `dir`/`env_file` inside the project; env keys `[A-Za-z_][A-Za-z0-9_]*`; max 8); `default_agent` must be an agent profile; `default_shell` must be a shell profile (same absent / `null` rules); `jira_keys` (string list) replaces the list, `[]` clears it, `null` = unchanged; each key is trimmed, upper-cased and de-duplicated (first-seen order), and must match `^[A-Z][A-Z0-9_]+$` (`invalid_request` naming the key; ADR 0066); `color` must be one of red, orange, yellow, lime, green, teal, blue, purple, pink (`null` = none)); `archive`: `project_id` → Project (`archived: true`, also `project.updated`): its live sessions are asked to stop, clients leave archived projects out, `add` of an archived project's folder un-archives it (same id, tasks and history) — ADR 0019 |
| `project.clone` | `clone_id` (client-chosen), `url`, `parent` (absolute folder) | `null` at once (ADR 0076); refused when the URL names no folder (`invalid_request`), `parent` is no folder (`invalid_request`) or `<parent>/<repository name>` exists and is not empty (`invalid_state`); then `git clone --quiet -- <url> <name>` runs in `parent` in the background (no terminal prompt, no console) and the clone is added like `project.add`; the end in `project.cloned` |
| `project.run_start` | `project_id`, `name?`, `cols`/`rows` (default 120×30) | `RunStarted { tab_id?, sessions: [{name, session_id, started}] }` — a run command (ADR 0018) runs as the project's live `shell`-kind session titled `run:<name>` (always the `shell` profile, not `default_shell`; ADR 0032): without `name` every command not already running starts; a running one is reported with `started: false`. cwd = `<project>/<dir>`; env = the shell profile's, then the env file (relative to the project folder), then `env`; the command is typed into the shell followed by an exit for that shell family. Every command is validated before anything spawns (missing folder, unreadable env file → `invalid_request` naming the command). The live run sessions lie side by side in the project's `run` tab (created when missing; layout rebuilt on every start, config order). No commands: `invalid_state`; unknown `name`: `not_found` |
| `project.run_restart` | `project_id`, `name?`, `cols`/`rows` | `RunStarted` — stops the targets like `run_stop` (5 s grace, then forced kill), waits until they exited (max 10 s, else `timeout`, nothing started), then starts them like `run_start`; the new session (a new id: a run is one session with its own log and History entry) takes the old pane's place in the `run` tab and keeps its color and pin |
| `project.run_stop` | `project_id`, `name?` | ok — asks the run sessions to terminate (without `name`: every live run session of the project, also one whose name left the config); ok when nothing runs. Each one leaves the `run` tab when it exits (§10) |
| `session.ports` | `project_id` | `[{session_id, ports}]` of the project's live sessions with ports (kept in daemon memory only) |
| `port.list` | – | `PortRow[]`: every listening TCP port of the daemon's machine (`port`, `addr`, `pid`, `process`, `project_id?`, `worktree_task_id?`, `branch?` without `mkrew/`, `session_id?`, `session_title?`, `run_command?`); a pid under a live session takes that session's workspace, worktree and run command; else a cwd inside a workspace or task worktree folder (the longest wins) takes it; the daemon and its own children (`mk web`) are left out (spec 2026-11-01, ADR 0091) |
| `port.stop` | `port`, `pid` | ok - lists again first; `not_found` when `pid` no longer holds `port`; a run command's session is killed like `project.run_stop`; any other process is killed with its descendants, never its parents, so a shell pane or an agent stays; `invalid_state` when the OS refuses, when the daemon is among the descendants (it never kills itself), or when `pid` is a session's own root process |
| `watch.release` | `project_id`, `worktree_task_id?` | ok (`null`) - the UI no longer shows that workspace (or task worktree): it gives back the file watcher that `doc.*`, `fs.*` and `git.status` keep running. The watcher stops unless a session of the project is live; a later request of those kinds keeps it again (ADR 0087) |
| `limits.get` | - | `AgentLimits[]`: the newest 5-hour and weekly limits each agent reported (`agent` = claude or codex, `five_hour?` / `weekly?` = `{used_percent, resets_at?}`, `observed_at` Unix seconds); kept in daemon memory only (ADR 0102) |
| `limits.report` | `AgentLimits` | ok (`null`) - from `mk statusline`; the daemon keeps the newest `observed_at` per agent (a tie: the later report) and announces `agent.limits`; `invalid_request` for an agent other than claude and codex or a percentage that is not a number; percentages are clamped to 0-100 |
| `task.list/create/update/move/delete` | `project_id`, `status`, `position` | Task (`list`: Task[], filters `project_id?`/`status?`; `create` = Todo card, `title`, `prompt`, `agent_profile?` (none = project `default_agent` at run time, stored as `''`), `position?` (after the last Todo card); `update`: `title`/`prompt`/`agent_profile`, absent = unchanged, `null` agent = default; `move`: `status` + `position`; `delete` returns ok, `invalid_state` while a session of the task is not exited, its exited sessions stay as history with `task_id` = NULL — ADR 0004) |
| `task.run` | `task_id`, `cols`, `rows` | `session_id` |
| `task.undo_preview` | `task_id` | `checkpoint`, `fingerprint` (opaque), `changed[]` (tracked files restored), `added[]` (tracked since the checkpoint, deleted), `created[]` (untracked files created since, deleted), `diff` (≤ 1 MiB) + `diff_truncated`; `invalid_state` without a checkpoint (ADR 0005) |
| `task.undo` | `task_id`, `confirm: true`, `created[]` + `fingerprint` (from the preview) | ok — restores the project folder to the checkpoint and deletes the previewed `created` files that are still new; `invalid_request` without `confirm`, `invalid_state` while a session of the task is live, without a checkpoint, or when the tracked changes no longer match `fingerprint` |
| `session.spawn` | `project_id`, `agent_profile?`, `kind`, `tab_id?`, `title?`, `cols`, `rows` | `session_id` — no `agent_profile` = project `default_agent` (agent) or `default_shell` (shell), `invalid_request` when that default is not set; a named profile sets the kind; a tab with `null` layout gets the session as its only pane |
| `session.list` | `project_id?` | Session[] (with `terminal_title?` and `tsp` from daemon memory) |
| `session.attach` | `session_id` | screen snapshot (grid + cursor + modes) + subscribe to output |
| `session.detach` | `session_id` | ok |
| `session.input` | `session_id`, `data` (base64) | ok |
| `session.send` | `session_id`, `text`, `submit` (bool) | `send_id`, `queued` (from M10, §20B.2) — queued if the session is working; multi-line text goes as a bracketed paste when the program enabled mode 2004 |
| `session.wait` | `session_id`, `until` (states), `timeout_ms` | final state |
| `session.resize` | `session_id`, `cols`, `rows` | ok |
| `session.kill` | `session_id`, `force` | ok |
| `session.remove` | `session_id` | ok — leaves the session out of `session.list` for good (row, log, turns kept); a live one is asked to stop first (ADR 0019) |
| `session.rename` | `session_id`, `title?` (trimmed; `null`/blank clears) | the Session, also broadcast as `session.updated` — `invalid_state` for a run session (named by its command) or one caught exiting, `invalid_request` for a title starting with `run:` (ADR 0031) |
| `session.pin` | `session_id`, `pinned` | the Session, also broadcast as `session.updated`; a live session through its task, an exited one in the store (`invalid_state` for one caught exiting); a resumed session keeps the pin (ADR 0017) |
| `session.move` | `session_id`, `position` (finite number; non-finite = `invalid_request`) | the Session, also broadcast as `session.updated`; like `session.pin` (live through its task, exited in the store); sidebar order inside the workspace group, ascending, 0 = never moved; a resumed session keeps it |
| `session.set_color` | `session_id`, `color?` (a workspace color name; `null` clears) | the Session, also broadcast as `session.updated` — `invalid_request` for a name outside the palette, `invalid_state` for one caught exiting (ADR 0037) |
| `session.history` | `session_id` | `files[]`: existing session log files, oldest first (`<id>.log.1`, `<id>.log`); works for exited sessions (also from before a restart); empty after retention deleted the log (ADR 0060). Windows read logs with `session.log_tail`; `session.history` serves `mk web` and the CLI on the daemon's computer |
| `session.log_tail` | `session_id`, `max_bytes` (capped at `READ_WINDOW`, 4 MiB) | `data` (base64 on the wire): the newest bytes of the session log (`<id>.log.1` then `<id>.log`), `cut`: older output was left out. Works for exited and hidden sessions; `not_found` for an unknown id |
| `session.transcript` | `session_id`, `offset` (0 on the first read) | `found`, `format` (the profile's `[usage] format` as written, `""` without `[usage]`), `start`, `len`, `data` (base64): one window (at most `READ_WINDOW`) of the agent's transcript file. From `offset` while the file still reaches it, else from 0; from 0 the last window. `found: false` with empty `data` when there is no profile `[usage]`, native id or file. omp only: `recap` (`{text, at}`, `at` in Unix seconds), the newest row of `session_recaps` in omp's `history.db` for the session (ADR 0105); absent for the other agents and when omp wrote none |
| `session.resume` | `session_id`, `tab_id?` | `SessionSpawnResult` — relaunches an exited agent session into its conversation with the profile's `resume` command, as a new session (same title, color, pin, plan, size; a task's session stays the task's, in its worktree); placed where the old pane still is, else into `tab_id` when that tab is empty (the client places it otherwise). `invalid_state` while the session is live, for a shell, without `native_session_id` or `resume`, in an archived workspace, or while a live session has the same `native_session_id` or task (ADR 0055) |
| `session.link_plan` | `session_id`, `path?` (a `.md` file, absolute or project-relative; `null` unlinks) | the Session (also `session.updated`) — live sessions only (`invalid_state`/`not_found` otherwise); `invalid_request` when the file is not `.md`, missing, a folder, over 2 MiB or not UTF-8; the daemon then watches that one file (ADR 0039) |
| `session.plan` | `project_id` | `[{session_id, name, done, total, items[], error?}]` of the project's live sessions with a plan (the last announced checklist, for a client that connects later) |
| `tsp.snapshot` | `session_id` | `{state, surfaces, blobs, reason?}` (ADR 0107): `state` is the session's `tsp`; `surfaces` = `[{id, mode, role?, title?, read_only, seq, root}]` (`mode` `inline` or `screen`, `seq` the last applied frame of omp, `root` the node tree `{id, k, p?, c?}`; at most 3 surfaces); `blobs` = `[{id, mime, data}]` (`data` base64) of the images an `image` node still names, at most 4 MiB; `reason?` is why the session fell back, for the toast. A session that is not TSP, or has no open surface, answers its `state` with empty lists; a session that is not live is answered as `session.attach` answers it. A reply that would pass the 16 MiB frame limit is sent without its blobs (an image then draws its alt text); if it is still too large the call fails with `internal` ("snapshot too large") and the connection stays open |
| `tsp.event` | `session_id`, `surface`, `event` | `null`: `event` is an object whose `ev` is one of `action`, `toggle`, `select`, `activate`, `focus`, `theme`; the daemon adds `sf` (the surface) and writes it to the agent's input as a TSP message. Another `ev` is `invalid_request`. A `theme` event (`{ev:"theme", dark}`) also becomes the `dark` of the next hello |
| `tab.pin` | `tab_id`, `pinned` | TerminalTab, also broadcast as `tab.updated`; `position` is left alone, so unpinning puts the tab back |
| `tab.set_layout` | `tab_id`, `layout` | ok (the daemon stores the JSON as given and validates nothing; leaves are sessions, files, the diff or web pages, §5; `cut` and `layout_sessions` read only session leaves) |
| `hook.report` | `mkrew_session_id`, `agent` (`claude`/`codex`/`omp`/`copilot`/`fake`), `event`, `payload` | ok — `not_found`/`invalid_state` unless the session is live; stored as a `hook` session event |
| `agent.profiles` | – | AgentProfile[] (each with `installed`: its program, or the OS shell for a shell without `command`, is found on PATH) + parse errors |
| `slash.list` | `profile_id`, `project_id`, `worktree_task_id?` | `cli?` (absent when the profile's CLI is unknown, then `items` is empty), `items[]` (`name` without `/`, `description`, `argument_hint?`, `source`: `built_in`, `project`, `user`, `managed`, `{"plugin": name}` or `profile`; `group?` on the omp `skill:` row), `errors[]` (files that could not be read); the `/` commands of the profile's agent CLI in the project folder, or the task's worktree, read on the daemon's machine |
| `doc.list` | `project_id` | `DocFile[]` sorted by path: md files matching review globs + md files in `files_changed` session events of the last 24 h + files with unresolved comments (existing files only); each with `open_comments` (unresolved thread roots), `last_session_id?`/`last_changed_at?` (newest such event). Keeps the project watched for `doc.changed` until released (`watch.release`) or the daemon stops |
| `doc.read` | `project_id`, `file_path` | `file_path`, `content`, `hash` (opaque); ≤ 2 MiB UTF-8; `invalid_request` for absolute/`..` paths. Also keeps the project watched (until released) |
| `comment.list/create/reply/resolve/reopen` | `file_path`, `start_line`, `end_line`, `quote`, `body` | DocComment (every comment carries `context`: `doc` (default) or `diff`, replies = their root's; `list`: `project_id` + `file_path?` + `context?` → DocComment[], roots and replies oldest first; `create`: `project_id`, 1-based inclusive lines, `context?` (`diff` = new-side lines of a diff); `reply`: `comment_id`, `body`, `mkrew_session_id?` (set by `mk comment reply` = agent author); `resolve`/`reopen`: `comment_id`). Ids are `c_` + ULID; the prefix is optional when parsing |
| `comment.send_to_agent` | `comment_ids[]`, `session_id` | the thread roots, now `sent` (`sent_to`, `sent_snapshot` = file content), request order; reply ids stand for their thread; all on one file of the live session's project and of one `context` (`diff` → the prompt asks to "Fix the code in <file>" instead of "Update <file>", without "Only edit this file"). The §12 prompt is queued like `session.send` (submit); multi-line sends are wrapped as a bracketed paste when the program enabled mode 2004 |
| `fs.list` | `project_id`, `dir`, `show_ignored` | children (lazy) with git status |
| `fs.read` | `project_id`, `path`, `index_text?` (bool) | content (≤ 2 MB), binary flag, language, `hash?` (opaque, set with `content`), `index_text?` (asked for: the file's text in the git index, ADR 0047); `path` is relative to the root, or an absolute path inside the daemon's absolute notes folder (`[notes] folder`, normalized, no `..`; ADR 0108); any other absolute path is refused as before |
| `fs.write` | `project_id`, `path`, `content`, `expected_hash` | `saved`, `hash`. Writes an existing file (≤ 2 MB text, in place) only if it still hashes to `expected_hash`; else `saved: false`, nothing written, `hash` = the disk's (ADR 0028); `path` follows the same rule as `fs.read`: root-relative, or absolute inside the notes folder (ADR 0108) |
| `fs.create` | `project_id`, `worktree_task_id?`, `dir?` (`/`-separated, `""` = the root), `name` (may hold `/` or `\`: missing folders are made), `folder` | `path` of the new empty file or folder, relative to the root. `already_exists` for an existing entry; `invalid_request` for an empty name, `.`/`..`/drive parts, or a `dir`/`name` outside the root or inside `.git` (checked before anything is created) |
| `fs.new_note` | `project_id`, `worktree_task_id?` | `FsCreated { path }` - creates an empty `note-YYYYMMDD-HHMMSS.md` (local time; `-2`, `-3`, ... while the name exists, up to `-1000`; when all 1000 names are taken the answer is `already_exists`) in the daemon's `[notes] folder` (§20.0: empty = the root of the project or task worktree, relative = under it, absolute = as is), making missing folders; `path` is relative to the root when the note is inside it, else absolute. `invalid_request` (the message names the reason) for a bad folder setting (`..`, `~`, an unusable drive, not a folder) and for a create failure (the OS message: no write access, disk full); `internal` when the blocking task fails to join; nothing is created on an error (ADR 0108) |
| `fs.ignore` | `project_id`, `worktree_task_id?`, `path`, `is_dir` | `null` — appends `/path` (`/path/` for a folder) to the root `.gitignore`, creating it, keeping its line endings, not twice. `invalid_request` for a path outside the root or inside `.git` |
| `fs.find` | `project_id`, `query`, `limit` | fuzzy file matches |
| `fs.search` | `project_id`, `worktree_task_id?`, `query`, `regex?` (default false), `limit?` (default 200) | `{hits: SearchHit[], truncated}`; a `SearchHit` is `{path, line, preview, ranges: [[start, end]]}` (byte ranges in `preview`, at most 200 chars of the line, trimmed); text files up to 2 MiB under the root, smart case (case-insensitive unless the query has an upper-case letter), `invalid_request` for an invalid regex; runs beside the read loop, and a newer `fs.search` on the same connection stops an older one, which answers `invalid_state` |
| `git.status` | `project_id`, `mode` (`working`/`base`), `since?` (base mode: compare from this commit instead of the merge-base, e.g. `task.checkpoint`; `merge_base` = it) | `branch?`, `head?`, `upstream?`, `ahead`/`behind`, `base_ref?` + `base_auto`, `merge_base?` (base mode); changed files (`path`, `old_path?`, `kind`, `added?`/`removed?`) in `staged`/`unstaged`/`untracked` (working mode; conflicts are `unstaged`) or `against_base` (base mode) |
| `git.diff` | `project_id`, `path`, `mode`, `staged?` (working mode: index side), `commit?`, `since?` (base mode, as in `git.status`) | `binary`, `too_large`, `added`/`removed`, `hunks[]` (`old/new_start/lines`, `header`, `lines[]` with `kind`, `text`, `old_line?`/`new_line?`, `no_newline`, `crlf`) |
| `git.viewed` | the params of `git.status` | `ViewedFile[]` (`path`, `staged`): the files of that list (`working` / `base` / the `since` checkpoint) whose viewed mark still stands (ADR 0065). Re-hashes the marked files through the diff cache and deletes marks whose diff text changed |
| `git.set_viewed` | the params of `git.diff` (no `commit`), `viewed` (bool) | `null`; hashes the file's diff now and stores (`viewed: true`) or removes the mark; `invalid_state` for a file with no diff, `invalid_request` with `commit` |
| `git.stage` / `git.unstage` | `paths[]` or `hunk` | ok; `invalid_request` for a hunk line holding a line feed (it would add sections to the patch) |
| `git.discard` | `paths[]` (empty with a hunk) or `hunk` (a `HunkTarget` of the unstaged diff), `confirm: true` | ok (destructive; UI always confirms first). A hunk is reversed in the working tree only (`git apply -R`, the index stays); `invalid_request` for `paths` together with a hunk, for an untracked file or for a hunk line holding a line feed; a hunk that no longer applies fails with git's text |
| `git.commit` | `message` | new commit hash |
| `git.log` | `limit`, `path?` | Commit[] |
| `git.show` | `commit` | commit info + changed files |
| `ai.generate` | `generation_id` (client-chosen), `project_id?` (required except for `ask`), `worktree_task_id?`, `agent_profile?` (none = project `default_agent`; required for `ask`), `kind` (`commit_message`/`enhance_prompt`/`ask`), `input`, `web?` (`ask` only, default false) | `null` at once; runs the profile's `oneshot` (§6) with a 120 s limit in the project/worktree folder, prompt on stdin; `ask` runs `ask` (else `oneshot`), or `ask_web` with `web`, in `<data>/mkrew/scratch` with the question as the prompt (ADR 0061); a missing command is `invalid_state` naming the line to add; answer in `ai.generated` |
| `git.set_base` | `project_id`, `base_ref` (`null`/blank = auto-detect; trimmed; refused if it starts with `-` or contains whitespace or control characters) | ok — broadcasts `project.updated` (Project carries `base_ref`) |
| `git.fetch` / `git.push` | `project_id` | ok (push = current branch to its upstream, never force) |
| `git.repos` | `project_id` | `string[]`: `""` when the project folder is in a git repository, then each direct child folder holding `.git` (folder or file), sorted; hidden folders, `node_modules` and `target` skipped; not filtered by `.gitignore`; read on every call (ADR 0029) |
| `git.branches` | `project_id`, `worktree_task_id?`, `repo?` (ADR 0029) | `Branch[]`, newest commit first: `{name, remote, current, upstream?, last_commit_at, subject}`; `refs/remotes/*/HEAD` and remote branches that have a local branch are left out; `invalid_state` outside a repository |
| `git.checkout` | `project_id`, `worktree_task_id?`, `repo?`, `branch` | `null`; `git switch <branch>`, for a remote branch `git switch --track <remote>/<name>`; git's stderr is the `invalid_state` message; broadcasts `git.changed` |
| `daemon.status/stop` | – | info / ok; `status` carries `web` = `{enabled, running, error?}`: the `mk web` child the daemon supervises while `[web] enabled` (§20.0, ADR 0053); `error` = the child's last stderr line |

`git.status`, `git.diff`, `git.stage`/`git.unstage`, `git.discard`, `git.commit`, `git.log`, `git.show`, `git.fetch`/`git.push`, `git.viewed`, `git.set_viewed`, `git.branches`, `git.checkout` and `ai.generate` accept `repo?`: one folder name from `git.repos`; the request runs in that child repository. Paths in requests and replies stay relative to the project folder (`fe/src/a.ts`); a path outside the repo, a bad name or `repo` together with `worktree_task_id` is `invalid_request`. `git.changed` has no repo: clients refresh the shown repo (ADR 0029).

**Events**

| Event | When |
| --- | --- |
| `session.output` | **binary frame** (see below): raw PTY bytes, only to attached clients, batched ≤ every 16 ms |
| `session.snapshot` | only to an attached client whose output queue overflowed: fresh screen snapshot replacing the dropped output (backpressure, below) |
| `session.state` | session state changed (`state`, `source` = hook/screen/daemon, `changed_at`) |
| `session.exited` | process exited, with `exit_code` |
| `session.ports` | a run session's port list grew (`session_id`, `ports`): ports are scanned from its screen text (`http(s)://<host>:<port>`, `port(s)[:=] <n>`), first 4 distinct, never removed while it lives |
| `session.usage` | an agent session's token usage changed at a turn end (§20E.3): `session_id`, `input` (fresh, cache excluded), `output`, `cache_read`, `cache_write`, `context`, `context_window?`, `model?`, `transcript?` |
| `agent.limits` | an agent's newest limits changed (`AgentLimits`, see `limits.get`); Codex's come from its transcript at a turn end (§20E.3), Claude's from `mk statusline` |
| `task.updated` / `project.updated` / `tab.updated` | entity changed |
| `session.updated` | a session's title, color, pin, position or plan link changed (`session.rename`, `session.set_color`, `session.pin`, `session.move`, `session.link_plan`), or its `tsp` state changed (ADR 0107): the whole Session |
| `session.terminal_title` | an agent session's terminal title (OSC 0/2) changed: `session_id`, `title?` — leading glyphs stripped, paths ignored, ≤ 60 chars, once per change; memory only (ADR 0031) |
| `tsp.update` | omp's surface changed (ADR 0107): `session_id`, `surface`, `change` = `{kind:"frame", seq, ops, blobs?}` after the daemon applied a frame of omp (`ops` are the ops that applied, plus the daemon's own eviction ops; `blobs` the images the frame needs), `{kind:"open", surface}` when omp opens a surface, or `{kind:"closed", keep}`. Updates arrive in order, one `seq` after the other; a client that sees a gap asks `tsp.snapshot` again. The `mk web` relay does not forward it |
| `session.plan` | the checklist of a linked plan changed (`session_id`, `name` = file name without `.md`, `done`, `total`, `items[]` = first 200 task items `{done, text}` (text cut at 120 chars with `…`), `error?` = why the file cannot be read, counts then 0): sent at once on link and ≤ 1 s after each change of the file (debounced), never twice with the same content; `-`, `*` or `+` items (`[ ]`/`[x]`/`[X]`) outside code fences (ADR 0039) |
| `task.deleted` | task removed (`task_id`, `project_id`; ADR 0004) |
| `conflict.detected` | two sessions edited the same file within 60 s (M5): `project_id`, `path`, `session_ids`, `task_ids` |
| `doc.changed` | a watched md file changed (`project_id`, `file_path`, new `hash`, `null` = deleted); its comments are re-anchored right after (`comment.updated`) |
| `comment.updated` | comment created/replied/state changed (incl. agent replies via CLI) |
| `notification.clicked` | user clicked an OS notification on Linux (UI focuses that pane; Windows uses a `mkrew://` link, §8) |
| `git.changed` | workspace git state changed (debounced: 300 ms quiet, at most 2 s after the first change, 5 s while an agent session of the project is `working`; a batch with nothing to report after filtering sends nothing) |
| `ai.generated` | answer to an `ai.generate` (`generation_id`, `project_id?` = the request's, `text?` trimmed and, except for `ask`, unfenced, `error?`) |
| `project.cloned` | end of a `project.clone` (`clone_id`, `project?` the added workspace, `error?` git's message) |

**Framing.** Every frame = `u32` big-endian length + `u8` kind + payload. Kind `0x01` = JSON
(requests, responses, events). Kind `0x02` = terminal output: 16-byte binary ULID of the session
followed by raw PTY bytes — no JSON, no base64, because agents and builds can print tens of MB.
The length counts the kind byte plus the payload; frames longer than 16 MiB are rejected as a
protocol error (the connection is dropped). Small inputs (`session.input`) may stay base64-in-JSON.

**Backpressure.** Each attached client has a bounded output queue (e.g. 4 MB). If it overflows
(slow or frozen UI), the daemon drops the queued bytes and sends a fresh screen snapshot instead.
The daemon never buffers unboundedly and never blocks a PTY reader on a slow client.

**Screen snapshot.** `session.attach` returns `{state, snapshot}`; `session.snapshot` carries
the same `snapshot`. A snapshot is the visible screen of the daemon's emulator plus its history
(ADR 0030): `cols`, `rows`, `lines` (one array per row of style runs `{text, fg, bg, flags}`,
trailing default blanks omitted), `history` (the rows above the screen in the same format, oldest
first, at most the daemon's 1,000 lines; empty and omitted on the alternate screen), `cursor`
`{row, col, visible}` and `modes` (`alt_screen`, `app_cursor`,
`app_keypad`, `bracketed_paste`, `mouse`, `sgr_mouse`, `focus_events`). Colors are `"default"`,
`{"indexed": n}` (0–15 = ANSI) or `{"rgb": [r, g, b]}`. `flags` bits: bold 1, dim 2, italic 4,
underline 8, inverse 16, hidden 32, strikeout 64, wide 128 — a run is all single-width or all
double-width (`wide`) chars. Output frames sent after a snapshot continue from it. Painting a
snapshot scrolls `history` into the client terminal's scrollback before it draws the screen.

Rules: on
`protocol_version` mismatch the daemon replies `version_mismatch` and the UI asks to restart the daemon.

v2 adds requests and events in §20; each milestone that changes the protocol bumps `PROTOCOL_VERSION`.

Protocol 40 (ADR 0053): `daemon.status` gains `web`.

Protocol 49 (ADR 0087): request `watch.release`.

Protocol 50 (remote machine, spec 2026-10-29 §5): requests `session.log_tail`, `session.transcript`, `fs.create` and `fs.ignore`.

Protocol 51 (port manager, spec 2026-11-01): requests `port.list` and `port.stop`.

Protocol 52 (slash picker, spec 2026-11-04): request `slash.list`.

Protocol 53 (ADR 0095): `attachment.save` takes `owner` (`{"session": id}` or `{"project": id}`) instead of `session_id`.

Protocol 54 (Polish spec 2026-11-21 §8): requests `git.branches`, `git.checkout`, `fs.search`.

Protocol 55 (Widget bar spec 2026-11-22 sections 5 and 6, ADR 0102): requests `limits.get` and `limits.report`, events `agent.limits` and `machine.stats`.

Protocol 56 (ADR 0103): `git.discard` takes `hunk`.

Protocol 57 (ADR 0105): `session.transcript` result gains `recap`.

Protocol 58 (ADR 0106): event `machine.stats` and `MachineStats` removed; `agent.limits`, `limits.get` and `limits.report` stay.

Protocol 60 (ADR 0108): a tab layout's leaves can be files (`{"file": "..."}`) and the diff (`{"diff": true}`) as well as sessions; request `fs.new_note`; `fs.read` and `fs.write` accept an absolute path inside the daemon's notes folder.

Protocol 61 (ADR 0107): requests `tsp.snapshot` and `tsp.event`, event `tsp.update`, and `Session.tsp` (`off`, `waiting`, `live`, `fallback`; daemon memory only, like `terminal_title`).

Protocol 62 (ADR 0111): a tab layout's leaves can be browser panes (`{"web": "<url>", "id": "<ulid>"}`); an older UI would drop the leaf and write the layout back without it.

Protocol 41: `session.move` and `Session.position`.

Protocol 45: `Project.jira_keys`, `project.update` `jira_keys?` (ADR 0066).

Protocol 46 (ADR 0065): `git.viewed`, `git.set_viewed`.

Protocol 47 (ADR 0069): `Project.position`, `project.update` `position?`.

---

## §8 Daemon

- **Startup:** take the lock (exit if another daemon runs); run migrations; mark sessions left in working/needs_input/idle as `exited`; run retention (ADR 0060) and `VACUUM` the database when over a quarter of it is free; open the IPC endpoint;
  then resume (below).
- **Spawn:** create the PTY with `portable-pty`, `cwd` = project path, env `MKREW_SESSION_ID`,
  `MKREW_SOCKET`, `MKREW_MK`, `TERM=xterm-256color`. The daemon removes `OMPCODE` from the PTY environment,
  so a daemon started from an omp shell does not make the omp extension turn itself off.
- **PTY reader threads:** `portable-pty` reads are blocking, so each session gets ONE dedicated OS
  thread that reads into a buffer and forwards chunks over a bounded channel to tokio. Never call
  blocking PTY reads inside async tasks.
- **Headless emulator per session:** the daemon feeds every chunk into its own
  `alacritty_terminal` instance sized to the session, with a bounded scrollback (1,000 lines,
  ADR 0082). It is used for (a) state detection on the real screen text and (b) instant attach
  snapshots, which carry those lines (ADR 0030).
  Memory budget: a cell costs about 24 bytes (≈ 2.9 MB per full 120-column history), so the
  daemon scrollback stays bounded. Long history lives in the session log file on disk.
- Output fan-out per chunk: emulator → state detector (at most 4 scans/s per session, ADR 0088),
  session log file (buffered writer), attached clients (binary frames, §7).
- **Command lookup:** on Windows try `PATHEXT` (`claude.cmd`, `.ps1` shims).
  Clear error if not found.
  On macOS the `PATH` is the login shell's, merged before the process's own (`mkrew_core::login_path`, §20L).
  So Homebrew/npm installs are found, also by an app opened from Finder, the Dock or non-interactive ssh.
- **State detection:** one state machine per session (lives in `mkrew-core`, pure, unit-tested).
- **Send queue:** `session.send` while the session is `working` is queued and delivered when the
  session reaches idle/done/needs_input.
- **Resume:** on startup, for each task in running/needs_input whose last session has a
  `native_session_id` and whose profile has `resume`, spawn the resume command. Otherwise set the
  task to needs_input with the note "session stopped — run again?". Then, for each session marked
  `interrupted` (it exited during the daemon stop, or was still live at startup after a
  crash/reboot) that has no task, is an agent, and is still a leaf of a tab layout: spawn the
  resume command in the same pane. One that cannot resume stays exited. The mark is cleared at
  startup, so each session is resumed at most once. A resumed session keeps its linked plan
  (`plan_path`). Then every interrupted session that did not come back (a shell, a run command,
  an agent that cannot resume) leaves its tabs; a tab left without panes closes unless pinned,
  and interrupted shells and run commands are hidden like `session.remove`, while agents stay in
  History (archived workspaces are skipped; ADR 0093). A session that exits while the daemon runs
  (a shell or agent that quits, a killed one, a run command) leaves its tab layout; one cut off by
  the daemon stop keeps its pane until that startup step (ADR 0017, ADR 0033). Any exited agent
  session can also be resumed on request: `session.resume`, History's **Resume**,
  `mk session resume` (ADR 0055).
- **Run commands:** after resume, `project.run_start` (all) for every non-archived project with
  `run_autostart` and at least one command (ADR 0018).
- **Notifications:** OS notification when a session enters needs_input or done; at most one per
  session per 30 s. Clicking it shows that session's pane: on Windows the toast opens
  `mkrew://session/<id>?project=<id>`, which the user's daemon registers under HKCU for the
  `mkrew.exe` beside it (the running UI shows the pane, or a new UI starts on it, ADR 0079); on
  Linux it emits `notification.clicked`. While `[web] push` is on, `mk web` also
  sends a Web Push notification to each phone that opted in, under the same `[notify]` switches and
  the same 30 s limit (ADR 0067; §20.0).
- **Session logs:** `<data>/mkrew/sessions/<session_id>.log`, rotated at 5 MB; deleted 7 days after the session ended unless a tab still shows it (ADR 0060).
- **Retention:** hourly (and at startup): logs as above; `session_event` rows of kind `files_changed`, `hook`, `edited` older than 7 days. Session rows, turns and `usage` events stay.
- **Project watcher:** skips `.git`, `node_modules`, `target` and `.worktrees/`; honours every `.gitignore` from the project root down and `.git/info/exclude`, with git's precedence (the deepest level with a match decides each path prefix; nothing inside an ignored folder is reported or read, except Markdown, which stays reported when gitignored (not under `.git`, `node_modules`, `target`, `.worktrees`) for doc review). One `files_changed` row per batch, stored on the project's newest live session.
- **Backups:** daily copy of `mkrew.db` to `<data>/mkrew/backups/`, keep 7.
- **Shutdown:** `daemon.stop` asks every PTY to terminate, waits ≤ 5 s, then kills.
- **Logging:** daily rotating files; level via `MKREW_LOG`. Panics are logged with backtrace.
- On Windows, start detached with no console window (`CREATE_NO_WINDOW | DETACHED_PROCESS`).

---

## §9 CLI (`mk`)

```
mk daemon start | stop | status
mk project add <path> [--name N]
mk project list
mk project update <project> [--base develop | --auto-base] [--run-autostart on|off] [--jira <KEYS>]
mk project run <project> [name]      # runs the run commands in the "run" tab (prints `<name> <session id>`)
mk project restart <project> [name]  # stops, then runs them again
mk project stop <project> [name]     # stops them
mk ports [--json]                      # listening TCP ports of the daemon's machine: port, workspace, main or worktree branch, source, pid
mk ports stop <port> [--pid <pid>]     # stops the process on that port (a run command's session ends); --pid when several listen
mk task add <project> "<title>" [--prompt "..."] [--agent claude]
mk task list [--project P] [--status S]
mk task run <task-id>
mk task undo <task-id> [--yes]         # prints files + diff since the checkpoint; --yes discards
mk session list [--project P]
mk session history <session-id>        # prints the session's raw output log (also exited ones)
mk session resume <session-id>         # relaunches an exited agent session's conversation; prints the new id
mk session spawn <project> --agent <profile>   # prints the new session id
mk attach <session-id>                 # raw-mode attach; detach with Ctrl+]
mk web [--listen ADDR] [--host NAME] [--new-token]  # phone page over the tailnet (ADR 0052); --new-token also drops push subscriptions (ADR 0067)
mk send <session-id> "<text>" [--no-submit]
mk wait <session-id> --until idle,needs_input,done [--timeout 60000]
mk integration install|uninstall|status <claude|codex|omp|copilot>
mk hook <agent> <event>                # called by agent hooks, not by users
mk statusline                          # Claude Code's status line command (ADR 0102): reports the rate limits, then runs your own status line; set up by `mk integration install claude`
mk open <project> [file[:line]]        # opens in Zed
mk comment list [--file <path>] [--project P]  # default project: MKREW_SESSION_ID's, else cwd's
mk comment reply <comment-id> "<text>" # used by agents to answer review comments
mk proxy                               # stdin/stdout ↔ daemon socket, for machine windows over ssh (ADR 0089)
```

`mk` resolves the current session from `MKREW_SESSION_ID` when relevant (e.g. `comment reply`
records the agent as author). `mk attach` makes Mkrew usable without the UI, including over SSH.

---

## §10 UI: layout, workspaces & terminals

One Space window with the sidebar (or one Space window per workspace, see "Window modes", ADR 0115), plus any number of detached windows holding terminal tabs (ADR 0096).
A title bar on top spans both columns (Windows and macOS draw it; Linux keeps the OS
title bar and shows the same bar under it, without window controls; ADR 0034):

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ ◧ A api ▾ │ [▣ ● tab ✕] [>_ ○ tab ✕] [▤ a.rs ● ✕] [± Diff ✕] +  ⚠ Claude 5h 84% (● 2 running ▾) ▦ ▤ ◫ ◨ ─ □ ✕ │
├──────────────┬────────────────────────────────────────────────────────────────────────┤
│ [⌕ Filter… /]⇲+│ ┌ the terminal pane, no header ──────────── ⤢ Pane zoomed ┐              │
│ [All│Working│Needs you]│                                                              │
│ ▾ A api (● 1 working)│  Todo │ Running │ Needs input │ …                              │
│ │ ✻ fix login    2m │  …or the terminal tabs/panes                                    │
│ │ RUN               │  …or the Markdown review view                                   │
│ │ ● FE      :5180   │                                                                 │
│ ▸ W web       1 idle│                                                                 │
│ ▸ 2 empty workspaces I D │                                                            │
│ 📥 Inbox clear     ⇤ │                                                                │
└──────────────┴────────────────────────────────────────────────────────────────────────┘
```
ADR 0054 simplified this screen: one place per piece of information, color for status only,
empty things hidden, detail on hover / in menus. It removed the old status bar; ADR 0102 added a thin widget bar along the bottom and ADR 0106 removed it again: there is no bottom bar, the terminal tabs live in the title bar, and the agents' limits show as a chip in it (see "Limits chip" below).
The right panel (below) is a third column right of Terminals.
Its tab row is `[Files│Session│Git]`; files and the diff open as tabs and panes of Terminals (ADR 0108), so the title bar has no Editor or Diff chip.

- **Sidebar:** workspaces; expanding one lists its live sessions with status (§11); a session that
  exited leaves the list and its pane leaves its tab (ADR 0033; History keeps it).
  A **filter box** (`⌕ Filter sessions…`) shows above the All / Working / Needs you control only while it has the keyboard or holds text.
  `/` (while no terminal or field has the keyboard) and Alt+0 show and focus it; Esc on an empty box hides it and gives the keyboard back; `×` inside it clears it while it holds text.
  It matches workspace name, agent and
  session title as you type (every word must match; client-side, not remembered; while it holds
  text ↑/↓ move a highlight over the matching session rows, no wrap, and Enter opens the
  highlighted one, the first by default, or the first workspace when no session matches; with an
  empty box Enter gives the keyboard back to the pane).
  Alt+0 (§13) focuses the box with the active row highlighted: with an empty box ↑/↓ walk every visible row, ←/→ fold / unfold the highlighted row's workspace and Enter opens it; with text ←/→ move the caret.
  Tab / Shift+Tab in the box step the All / Working / Needs you chips.
  The sidebar has no row of buttons above the list: "Remove idle agents and terminals" and "Collapse all workspaces" / "Expand all workspaces" are palette entries (§13), and workspaces are added from the switcher, the palette and the Add workspace command (§13).
  Without workspaces the sidebar keeps its "Workspaces" header with `+` and the empty state, whose primary action adds one.
  The segmented control `All N`, `Working N` and `Needs you N` (N amber while
  above 0) is the top of the sidebar and narrows the list (the numbers count the sessions matching the filter text). *Needs
  you* is what the Inbox listed before: needs_input, plus done and not yet viewed (ADR 0036).
  While a filter or chip is on, only workspaces with a match show, open.
  Each workspace is a card (ADR 0062): `card` fill, faint border, radius 8, 8 px between cards,
  a 3 px stripe of the workspace color on its left edge (none without a color). Its header row:
  chevron, colored letter avatar, bold name (card-title size), 📌, then its status: a
  `● 1 working` pill (a still dot) while an agent works, else muted `2 idle`, plus amber `? N`
  while N need input; the ⋯ menu on hover (or right-click). Its sessions sit indented in the
  card, agents first without a label (TERMINAL is labeled beside agents or RUN): agent glyph
  (in an accent chip while working; its hover says the state), title
  (ellipsized), the glyph of a waiting / done agent (none while idle), the time, ✕ on hover; the
  selected row is ringed. The shown workspace's header is filled only while none of its rows
  is the selected one (collapsed, filtered out, or no session focused; ADR 0068), so one
  highlight marks where you are. Run sessions are not session rows: a **RUN** group under the
  workspace lists each run command (green dot running, hollow gray stopped, red exited), its
  name and its ports in green monospace (folded, its label shows the ports); a running one
  opens its pane on click (right-click: pane menu, ✕ on hover), a stopped one has ▶ on hover.
  Workspaces with no session and no run
  command (except the shown one and pinned ones) sit on one line at the bottom,
  `N empty workspaces ▸` with their tiny avatars; a click lists them as normal rows.
  The **footer** (inside the sidebar's width) has the Inbox state (`Inbox clear`, or amber
  `N need you`; a click = Jump to next, as Ctrl+Shift+I), the voice input state while there is
  one (§20G; shown in the title bar while the sidebar is hidden) and the compact toggle.
  There is no Inbox card: the chips, the footer and Ctrl+Shift+I replace it; hovering a
  session row still peeks it (§20A). Toggle sidebar (Ctrl+Shift+B, the title bar's `◧`, palette) hides it.
  The layout is remembered as `[ui] sidebar = "full" | "compact" | "hidden"` (default full),
  written by both toggles and read at start (a peek or menu showing it for a moment is not saved).
  Toggle compact sidebar (Ctrl+Alt+B, the footer's `⇤` / `⇥`, palette) narrows it to a rail:
  workspace avatars with `?` or the working count, and per open workspace its sessions' status
  mark + agent chip; hovering a rail row shows its full name, status and folder as a tooltip
  (instead of the peek). Entering the rail clears the filter and chip; a hidden sidebar comes back
  at the width it had, and Ctrl+Alt+B on a hidden sidebar shows the rail. Not remembered
  across restarts (ADR 0040). Drag the full sidebar's right edge to resize it (200–480 px, at most
  half the window; saved as `[ui] sidebar_width` when the button is released).
- **Plan progress:** the selected session row of a session with a linked plan shows a thin
  progress bar (done / total of its checklist) under its name line (ADR 0039).
- **Remove:** the workspace menu's "Remove workspace…" archives it after a confirmation
  (`project.archive`, ADR 0019); a session row's ✕ (on hover) removes a live session from the
  sidebar (`session.remove`; it asks first and the session is stopped). A session that exits needs
  no removing (ADR 0033).
- **Color:** the workspace menu's swatches give a workspace one of 9 colors, shown as a letter
  avatar tinted with the color before its name and a 3 px stripe on its sidebar card
  (decoration only, ADR 0020, ADR 0027, ADR 0062).
  A pane has its own color too: right-click its tab, Change color (the sidebar row menu has it as well; same palette; ADR 0037).
  A colored pane has a 2 px line of its color at its top.
- **Pin:** the workspace menu, the pane menu (right-click a sidebar session row; Shift+F10 on a terminal) and the tab menu have Pin / Unpin
  (the palette has "Pane: Pin / Unpin" for the focused pane; `project.update` `pinned`, `session.pin`, `tab.pin`, §7). A
  pinned workspace leads the sidebar, a pinned session leads its workspace's list and a pinned tab
  sits at the left of the tab area; each group keeps its usual order (workspaces by dragged order, then age;
  sessions newest first, tabs by position), and unpinning puts the item back in it. A pinned item
  shows 📌 next to its name (after it in the sidebar, before it on a tab). Pins are stored (§5), so they survive a restart; a resumed session
  keeps its pin (ADR 0017).
- **Reorder:** drag a workspace card in the full sidebar to move it among the pinned or the
  unpinned workspaces (`project.update` `position`, the group numbered `1..=n`; ADR 0069), with
  the same drag look as tabs and rows; off while filtering and in the compact rail.
- **Views per workspace:** Terminals (a click on any tab: terminal, file or diff), Board and Docs (icons in the title bar, only with `[features] docs` for Docs; a second click returns to Terminals).
  There is no Editor view and no Diff view: files and the diff are tabs and panes of Terminals (ADR 0108).
- **Title bar** (42 px, ADR 0027, ADR 0034, ADR 0054, ADR 0106), one bar across both columns, left to right:
  in a machine window (ADR 0089) the machine chip (below); the sidebar toggle `◧`; the workspace avatar + name ▾ (a dropdown of the workspaces, click switches, plus "Add workspace…");
  the tab area; a drag space; the right group; on Windows the window controls.
  A 1 px rule (`title_rule`) sets the groups apart: between the left group and the tab area, between the `+` row and the drag space, and before the right group (the one before the window controls stays).
  A rule is never a drag or press target; a detached window draws the rules where its groups exist (no rule for a missing left group).
  - **Tab area:** the pinned tabs, a 1 px rule, the other tabs (terminal tabs, file tabs and the Diff tab alike), and the `+` row (below).
    The area scrolls sideways when the tabs do not fit; the active tab is scrolled into view; a chip shrinks to `TAB_MIN_WIDTH` before the area scrolls, `TAB_TITLE_MAX_WIDTH` is its maximum; the `+` row stays visible after the scrolled tabs.
    Only the drag space moves the window, so a click on a tab never does.
    A tab shows a status icon, pane icons (see Terminals below), 📌 when pinned, its title and ×; the active tab has a solid `tab_active` fill (never equal to `chrome`, a `theme::problems()` rule) with radius 8 and no border, the others are transparent with muted text and a hover fill; a click activates the tab and shows Terminals.
  - **`+` row** (ADR 0108): the button is a `+`.
    On hover or keyboard focus it grows to the right, on the same line, in 150 ms ease-out (at once with reduce motion), into `[+] [default agent] [default terminal] [new note] [▾]`.
    It collapses 150 ms after the pointer leaves both the `+` and the row, so the pointer can travel to the items.
    The items are the default agent profile's logo (tooltip "New <agent> tab" with Ctrl+Shift+A; hidden when no profile is installed), `square-terminal` (tooltip "New <shell> tab" with Ctrl+Shift+T; the workspace's default shell), `notebook-pen` (tooltip "New note" with Ctrl+Alt+N) and `▾`.
    A click on an agent, shell or note item opens a new tab right after the active one; a drag starts the same thing as a leaf at the drop edge of a pane (see Drops below) or a new tab on the tab area.
    `▾` (tooltip "More…") opens the menu of before: one row per installed agent profile (the workspace's default first with ✓ and Ctrl+Shift+A), "Agent" when none is listed, Shell (Ctrl+Shift+T), Empty tab; each row opens a new tab; it is not draggable.
    Keyboard: Tab reaches `+`, ← / → move between the items, Enter picks, Esc collapses.
    While a session starts the row shows the starting spinner (loading dots) instead.
    Every item is a `Look::Icon` button with a focus ring.
    Alt+Shift+A and Alt+Shift+S add a pane to the current tab instead.
  - **Right group:** the voice status while the sidebar is hidden, the "need you" chip (a window per workspace only, see "Window modes"), the limits chip (Space windows, only at 80% or more, see "Limits chip"), the Run pill, the update button (only when an update is known), the toggles of the focused agent pane (below), Board (`SquareKanban`), Docs (`BookOpen`, only with `[features] docs`), Split ▾ (`◫`: Split right, Split down, Equalize panes, layouts, then "Open each workspace in its own window…" or "Combine into one window"; in a Space window only while Terminals show, a detached window always has it) and the right panel toggle (`◨`, ADR 0038, ADR 0081).
    Board and Docs show in the accent color while their view is shown.
  - **Pane toggles** (ADR 0106; only while an agent pane has the focus and its terminals are on screen; they act on that pane): Terminal | Chat (`MessageSquare`, only for a pane that can chat, accent while Chat shows, Ctrl+Alt+T and the palette "Agent: Toggle chat view"; new agent panes follow `[ui] chat_default`), Reading width (`AlignCenter`, accent while on, the palette "Pane: Reading width (on / off)") and, only with `[features] share`, Share (`Share2`, accent while following; its menu has "Follow in Zed").
  - History (palette "Global: History…"), the theme toggle (Ctrl+Shift+J, palette, Settings > Appearance), Phone access (palette "Global: Phone access…"), the branch chip and repository menu (the Git panel, Ctrl+Shift+4) and the logo are not in the bar.
  - Every icon is gray (`text-muted`, `text` on hover); color is for status (the tab's hourglass and bell), an agent's brand logo in its tab chip and the accent of a toggle or view icon that is on.
    The Git tab's count comes from `git.repos` and one `git.status` per repository, asked for the shown workspace only, on a workspace switch and on its `git.changed`; no polling, nothing for hidden workspaces.
  - Window: the empty part (the drag space) is the drag area; it keeps a minimum width of 32 px so the window stays movable with many tabs.
    **Windows:** double-click maximizes or restores, ─ □ ✕ at the right (hovering □ shows the Windows 11 Snap Layouts flyout), all edges resize; ✕ and Alt+F4 hide the window to the tray while `[ui] close_to_tray` is on (§20J); the controls are hidden in full screen.
    **macOS:** native traffic lights at the left, centered in the bar at any `[ui] font_size` (no gap is left for them in full screen), drag and double-click as the OS does.
    **Linux:** the OS title bar stays. Minimum window size 1100 × 560.
    Without a workspace only the sidebar toggle, the limits chip, the drag space and the window controls show.
- **Machine windows** (ADR 0089, `mkrew-ui --machine <name>`; this computer's window is unchanged):
  - The OS window title is `Mkrew - <name>`.
    The **machine chip** is the first element of the bar (after the macOS inset): Lucide `server`, the machine name (cut with `…` at `TITLE_CHIP_MAX_WIDTH`) and a status dot.
    It is not clickable; its tooltip says `<name>: <state>` in words (§11), and it stays while the window is disconnected.
    Dot: `status-done` connected, `status-needs-input` connecting or reconnecting, `status-exited` not reachable or another version.
  - **First connect:** the "Connecting…" screen names the machine.
    **Unreachable** (before the first `hello`, from ssh): the machine name, the reason, one hint (spec 2026-10-29 §4.4), the seconds to the next try (1, 2, 5, 10 s, then every 30 s) and **Try now**.
    **Version differs:** both protocol numbers, no retry, **Try now** to check again after updating.
    `ssh` runs with `ConnectTimeout=10`, so an unreachable host fails within about 10 s.
  - **Connection lost after it worked** (a machine window, and this computer's window when its daemon restarts): the window keeps its workspace, tabs, panes and last frames, and shows a banner under the title bar: "Connection to <name> lost. Next try in N s." (N is fixed per attempt; "Reconnecting…" while one runs; "the daemon" stands for the name in this computer's window).
    A second line gives the last attempt's detail: the unreachable reason and hint, the error, or the version message.
    **Try now** is always there except while an attempt runs; only one attempt runs at a time.
    The panes, sidebar and title bar ignore keyboard and mouse; a dialog that was open stays open, and its confirm toasts "Not connected to <name>. Reconnecting…".
    On reconnect the window rebuilds from the daemon, shows the same workspace and re-attaches every visible pane (snapshot first).
    Files tabs with unsaved changes, a Docs comment draft, a Git commit message and a card's follow-up text are kept; Save uses the old hash, so a file changed meanwhile is refused as always.
  - **Typed paths:** where this computer's window opens a picker (Add workspace, Clone into, Link plan), a machine window asks for a path on the other computer in a text field ("Folder on <name>", "Clone into (folder on <name>)", "Plan file on <name>"); the daemon checks it and its refusal shows under the field.
  - **This computer only:** Zed, Follow, Open transcript, Explorer / Finder and Reveal stay in the menus and toast "Opens files on this computer only; <name> is another computer."
    Files dropped on a pane, or a file list pasted, toast "These files are on this computer; <name> cannot read them." and type nothing.
    Phone access and the in-app update are hidden; close-to-tray is off.
  - **Notifications:** the window shows OS notifications on this computer from `session.state`, with this computer's `[notify]` settings; the link is `mkrew://session/<session>?project=<project>&machine=<name>` and a click raises that machine's window.
    The other computer's daemon still notifies on the other computer.
- **No status bar** (ADR 0054, ADR 0106): its sidebar toggle is in the title bar, its Inbox state and
  compact toggle in the sidebar footer, its git HEAD in the Git panel's repository strip, the
  focused agent's context share in the composer, and the
  other usage fields (tokens, cache, model) on the Session tab's card.
  There is no bottom bar of any kind: the folder is in the palette ("Reveal in Explorer"), the Git branch in the Git panel, the version in Settings, the agents' limits in the title bar chip.
- **Board:** columns by task status, drag & drop. Card: title, agent, run time, session status.
  Todo cards have Run. Running cards have a **follow-up** input (sends via `session.send`; Esc gives
  the keyboard back and keeps a typed draft on the card, like the composer).
  New task: the empty board's button, or `New task` in the Todo column header.
  With `[jira] site` set, a `Tasks | Jira` switch heads the Board (§20I).
  On the Tasks side a `This workspace | All workspaces` switch heads the Board (ADR 0090; `[ui] board_all_workspaces` keeps it across restarts and is also a switch on Settings → General; the palette has "Board: Toggle This workspace / All workspaces").
  All workspaces shows the tasks of every listed workspace (archived ones are not listed) in the same five columns.
  Inside a column the cards go by workspace in sidebar order, then by position; each card names its workspace (avatar and name) above its title.
  A drag or Shift+←/→ moves a card among its own workspace's cards (positions are per workspace); dropped before another workspace's card, it lands after its own workspace's last card in that column.
  Clicking a card, its Changes and Enter open it in its own workspace; New task (and Ctrl+Shift+N) adds the task to the active workspace, and the button's tooltip names it.
  The arrows, Ctrl+Enter, `a`, `e` and Shift+F10 act on every card of the All workspaces board.
  Show Board (Ctrl+Shift+2) again switches Tasks / Jira, and the card keys walk the Jira rows.
  Shift+F10 on the selected card opens its menu: Approve, Undo…, Changes, Merge back…, Remove worktree…, Compare…, Run with…, Run with several agents… (each as its hover button; the buttons stay and call the same functions).
- **Terminals:** the tabs are in the title bar (above); each tab is a split tree of panes; each pane is a session, a file or the diff (ADR 0108) with no header of its own (ADR 0106): a terminal fills its pane, a file pane shows the editor (§12A) and the diff pane the diff (§12B).
  A file or the diff opens as a tab right after the active tab, or as a pane when dropped on a pane (below).
  Panes sit on a frame with an 8 px gap and no border; a stripe at the top edge marks the focused pane, a pane that needs input or a pane color (§14).
  Each file and the diff exist at most once per workspace, in one pane across all tabs and windows.
  Each tab chip starts with a status icon only when the user should notice (§11): an hourglass in `status-working` while an agent works, a bell in `status-needs-input` when a pane needs input (needs input wins); none for idle, done, exited and starting.
  Then come the pane icons, all 14 px: `square-terminal` in `text-muted` for a shell, `play` in `text-muted` for a run pane, an agent's brand logo (claude, codex, copilot, omp) in its brand color (`brand-claude`, `brand-codex`, `brand-copilot`, `brand-omp`; each 3:1 on `chrome` and `tab-active`, a `theme::problems()` rule), and an agent without a logo its glyph in `text-muted`.
  The active tab shows one icon per pane in pane order, at most three, then `+N`; an inactive tab shows only its focused pane's icon; a tab without panes shows none.
  Next come 📌 when pinned, the title and a ✕ (always on the active tab, on hover for the others); closing never
  activates the tab first.
  The chip's tooltip says the status in words.
  A file or diff pane adds its file-type icon or the diff icon (in `text-muted`) to the pane icons, and the tab's title follows the focused pane (§11).
  The `+` row (below) follows the tabs.
  The split button (title bar, right group) opens
  the layout menu: Split right, Split down, Equalize panes, then the layouts 2 columns, 2 rows,
  3 columns, 1 + 2 and 2 × 2, each with a small drawing and its shortcut (§13). Equalize panes
  gives every pane of a run of same-direction splits the same share (split right twice: 50/25/25
  becomes thirds; a stack inside a column counts as one column) and saves the layout. A layout rearranges the active tab:
  its panes keep their tree order (left to right, top to bottom within a column) and the panes it
  lacks start sessions of the focused pane's kind (as Split right), one after another; then the
  layout is saved once. A layout with fewer panes than the tab has is greyed (its tooltip says
  why); if a start fails, the sessions that started are placed as Split right places them.
  Every popover menu below opens by mouse or keyboard, takes the keyboard while it is open and gives it back when it closes (§13).
  A right-click on a tab opens the tab menu: Pin, Split right, Split down, Rename, Close,
  Close others, Close tabs to the right (both leave pinned tabs and make the menu's tab active;
  greyed when nothing would close); on the active tab also the focused pane's Rename pane, Close
  pane and, for an agent, Follow in Zed (✓ while on) and Turns…. The tab is
  activated first, so the menu acts on the tab it was opened on. Closing a tab stops its panes'
  sessions and removes them from the sidebar (ADR 0041); a running agent among them asks first
  (once for Close others / to the right). Drag a tab along the tab area to reorder it (`tab.move`),
  among the pinned or the unpinned tabs only; tabs and sidebar rows share one drag look: a small
  title chip follows the pointer, the item stays dim in place and an accent line marks the gap
  it lands in.
  **Drops** (ADR 0108): a pane has four drop zones, one per edge, and no zone in the middle: the edge nearest the pointer wins (ties: right, then bottom).
  The half of the pane that the new leaf will take lights up (the accent tint), and the drop splits the pane 50 / 50 there.
  Every draggable uses them: a file row of the Files tree (opens the file), a `+` row item (starts that agent, shell or note), a tab (merges its panes, keeping its layout) and a pane by the grip on its top edge (an 8 px strip that shows a grip on hover; only in a split tab), which moves the whole leaf, of any kind.
  A pane or tab dropped on the empty part of the tab area becomes a tab of its own; a file row or a `+` row item dropped there becomes a new tab right after the active one.
  A pane dropped on itself, or a tab on the tab that holds only that pane, does nothing; nothing drops while a dialog is open.
  A file row dropped on an agent pane's composer inserts the path (§20B.5) and is not accepted by any terminal.
  A pane moves within its tab or out of another one; an emptied tab closes after both layouts are saved (`tab.set_layout`, then `tab.close`).
  OS files dropped on a terminal pane type their paths (§20B.5); on a file or diff pane they open as new tabs.
  Sessions are `agent` (from a profile) or `shell` (a shell profile, §6: the workspace's default
  shell, picked in the workspace menu under "Default shell for new panes" like the default agent;
  a new shell pane without one asks the user to pick; ADR 0032).
  New panes open in the project folder. Closing a pane with a running agent asks: kill the
  session, or just hide it (still reachable from the sidebar). A pane whose session exits closes
  itself. Layout persists (`terminal_tab`). Drag the gap between two panes to resize them (each
  side keeps at least 10 % of the split; hidden while a pane is zoomed); the layout is saved once
  when the button is released.
  No hard limit on terminals; warn above 12 running sessions per workspace.
  **Reading width** (agent panes): the title bar's Reading width toggle limits the terminal to 960 px (≈ 120–140
  columns), centered, so the agent wraps there: the terminal element is narrower and the usual
  resize sends `session.resize` (nothing reflows its output). Agent panes start with
  `[ui] reading_width` (default `true`); the toggle holds per pane until restart. Shell and run
  panes are always full width.
- **Right panel** (ADR 0038, ADR 0081, ADR 0084): a column right of the Terminals view (the tab's panes) with
  tabs `Files │ Session │ Git` (icon tabs on the left of its tab row with the title in the tooltip, Tern style, ADR 0110; a click switches the tab; the `Git` tab shows the changed count). The title bar's `◨` and `Ctrl+Shift+Y` hide
  and show it and keep the tab; from the Board or Docs view the key shows Terminals with the
  panel open. `[ui] right_panel` (`"files"`, `"session"`, `"git"` or `"hidden"`, default hidden) remembers
  it across restarts. Drag its left edge to resize (240–560 px, one width for all three tabs, saved as
  `[ui] inspector_width` when the button is released); the terminals lose that width, so an agent
  redraws once. **Files tab:** the file tree of §12A. **Session tab:** the session of the active
  tab's focused pane (nothing for a file or diff pane); shown only while `[features] session` is
  true (default false: the tabs are `Files │ Git` and a saved `"session"` opens on Files).
  **Git tab:** the Git list (header, Working / Vs. base / History, filter, Changed by, changes, commit box); a file's diff shows in the Diff pane (§12B).
  **Session card:** agent, title, state + time, the usage of §20E.3 (context
  share, tokens, cache, model; rows without data are left out), **Open transcript** (the agent's
  `.jsonl` in Zed; disabled until the daemon located it) and **Stop** (`session.kill`; a running
  agent asks first, a shell does not; the pane closes when the session exits, ADR 0033).
  **Changes card:** one line per repository with changes, `M 23 ?3` (modified paths, untracked
  files) with **Review diff**, which opens the Git tab and the diff of that repository (§12B); a workspace
  folder without `.git` still lists its child repositories (ADR 0029). Clean repositories have no
  line (`Working tree clean` when none changed); no repository, no card. The counts are the
  shown workspace's git summary, refreshed on `git.changed`.
  **Plan card** (agent sessions, after Changes): `Plan <name> 6/20`, a progress bar and the
  checklist of the linked file's `- [ ]` / `- [x]` lines: the first unchecked item highlighted
  (two rows of context above it), at most 8 rows, `N more` for the rest. **Link plan…** (a file
  picker) links a `.md` file; **Change…** and **Unlink** replace or remove it. The link is per
  session and survives restarts (the resumed pane keeps it). The counts follow the file within
  about a second (the daemon watches only that file); a file that cannot be read shows the
  reason instead of the bar. There is no "Setup / Tasks / Finish" phase bar: progress comes
  from the file only, for any agent (ADR 0039).
  **Thread** (agent sessions whose profile has `[usage]`, after the cards, filling the rest of
  the panel): the session's transcript read like a chat. Each prompt sits on a card block, the
  agent's text shows as Markdown, and consecutive tool calls form one `▸ +N tool calls` row (a
  click gives one line per call: name + target). Thinking, tool output and sub-agent lines are
  left out. The file is read only while the panel shows the session: the last 4 MiB first (at
  most 20 turns; older ones give "Earlier turns: Open transcript"), then what was appended,
  checked every second. Closing the panel or focusing another pane stops reading. A muted
  "Working…" ends the list while the agent works. The list follows the bottom unless scrolled
  up (ADR 0057).
- **Terminal rendering:** cell grid from `alacritty_terminal`; 256-color/truecolor, cursor,
  selection (a drag; a double-click takes the word, a triple-click the line, and a drag after either extends by words or lines; a bracket selects through its partner) & copy (the copied text never ends with a line break), paste (bracketed), scrollback (UI-side, 1,000 lines like the daemon's, only for
  attached panes; the History replay pane keeps 10,000; ADR 0088),
  wheel (scrolls that history; a program that turned on mouse tracking, like Claude Code's
  full-screen view, gets wheel reports instead, SGR when it asked; a full-screen program with
  alternate scroll gets Up/Down; copy mode always scrolls the history),
  a trackpad's pixel scrolling counts half a line per line of travel when it goes to a program (wheel reports or arrow keys) and a full line when it scrolls the history, and a change of direction drops the left-over fraction (ADR 0099),
  search, resize → `session.resize`. Performance rules (mandatory — this is the hottest path):
    - Batch adjacent cells with identical style into one shaped text run per row (as Zed's
      terminal element does). Never one GPUI element per cell.
    - Re-layout only dirty rows; cache shaped rows otherwise.
    - Present at most once per display refresh; coalesce output that arrives between frames.
    - Panes that are not visible (other tab, other workspace, minimized window) are detached:
      no parsing, no layout, no rendering. Re-attach uses the daemon snapshot, so the pane's
      scrollback restarts from the daemon's ≤ 1,000 history lines (ADR 0030, 0082).
    - Use a monospace font with a precomputed cell size (width and line height are whole device pixels, ADR 0099); no per-cell measurement.
- **Open in Zed:** on task cards, the workspace menu, the file tree menu and Docs; runs
  `zed <path>[:line]`; on macOS also `/usr/local/bin/zed`, `/opt/homebrew/bin/zed`, `~/.local/bin/zed`
  and the tool inside `Zed.app`, and with only the app installed `open -a Zed.app <path>` (no line);
  clear error with install hint if none is found.
- **Open in File Explorer** ("Reveal in Finder" on macOS): in the workspace menu and in the file tree's right-click menu
  (§12A). It opens the OS file manager (Explorer, Finder, the Linux file manager): a folder is
  opened, a file is shown selected in its folder.
- **Terminal links:** while Ctrl (Cmd on macOS) is held over a terminal pane (agents and
  shells) the link under the pointer is underlined and the pointer is a hand; Ctrl+Click
  (Cmd+Click) opens it. Links are `http://` / `https://` URLs (opened in a browser pane, §20M) and file
  paths, relative (`./src/a.rs`, `crates/x/main.rs`) or absolute (`/home/u/a.rs`,
  `C:\x\a.rs`), with an optional `:line` or `:line:col`. A path is a link only if it names an
  existing file inside the pane's folder (the project folder, or the worktree of the pane's
  task); it opens in a file pane (§12A) with the caret on that line. Trailing `.,;:!?`,
  unbalanced closing brackets and quotes are not part of a link. `file(12)` (the path before
  the bracket is the link, without the line), `~/…` paths and links wrapped over two rows are
  not detected. Perf: the row text comes from the cached grid, for the hovered row only, while
  the key is held and the pointer moves to another cell; nothing runs while output is parsed
  or painted.
- **Copy mode** (`Alt+Shift+C`, ADR 0085): keyboard selection in the focused pane, on `alacritty_terminal`'s vi mode.
  While it is on the keys go to the UI, not the program, and a floating badge in the pane's top-right corner shows `Copy mode` with the key hints.
  `←↓↑→` / `h j k l` move the cursor; `w` / `b` / `e` by word, `0` / `$` line start / end, `g` / `G` top / bottom of the history, `PgUp` / `PgDn` a page.
  `v` / `Shift+V` start, switch or drop a character / line selection; entering copy mode clears a mouse selection.
  `y` or `Enter` copy it (a trailing newline of a line selection is trimmed) and leave; `Ctrl+Shift+C` copies it too and stays in copy mode; `Ctrl+Shift+L` sends it to an agent's composer and leaves copy mode; `Ctrl+Shift+F` opens the find bar, whose match moves the cursor.
  `Esc` drops the selection, or leaves copy mode when there is none (an empty selection from a click counts as none).
  Copy mode survives the pane losing focus.
  With copy mode on, voice text goes to the clipboard, not the terminal.
  Output that arrives meanwhile is held back (up to 1 MiB) so the view and the cursor stay on their text; leaving copy mode applies it and returns to the live screen.
- **Link hints** (`Alt+Shift+O`, ADR 0085): labels every link of the visible rows (the same detection as Ctrl+Click, so a path counts only when it names a file of the pane's folder) with `a`, `s`, `d`, `f`, … then two letters.
  Typing a label opens that link exactly like Ctrl+Click; `Esc` or any other key cancels.
  Output is held back while the labels show (up to 1 MiB, like copy mode).
  Any key, a wheel scroll, a click or the pane losing focus ends the hints and applies the held output.
  No link on screen: a toast says "No links on screen.".
- **Run pill** (ADR 0018, ADR 0054) in the title bar: while something runs a green pill
  `● 2 running :8080 :5180 ▾` (`run-pill-*` tokens; amber dot and `1/2 running` when some
  exited; a steady dot: a pulse would redraw the window for as long as a server runs, §15);
  otherwise a muted `Run ▾`. Ports: every distinct port of the live run sessions, config order,
  up to 4 then `+N`; tooltip `name — :port …` per command. A click opens the Run menu
  (mouse-driven, 280 px wide): `RUNNING IN <WORKSPACE>` (`RUN IN` while idle), one row per command (state
  dot, name, ports or state in words, ↻ on hover and a red ■ while it runs, ▶ otherwise; a click on the row
  or Enter stops or starts it, the buttons keep the menu open), then
  `Restart all` / `Stop all` while anything runs or `Start all` while nothing does, `Show Run
  tab`, `Run setup…` (with no command, saving there also runs). Only Stop is red. Run from
  all-stopped switches to Terminals and the `run` tab; Restart and per-command Run keep the
  view. A stopped command leaves the `run` tab at once; so does one that ends by itself when its
  `Close the pane when it ends` box is on (ADR 0063; off, the shell stays open showing the
  output) (the tab
  stays, empty after the last one). Restart puts the new session where the old pane was, so the
  tab shows one terminal per command, never an old one beside a new one. A run pane's title is
  `<name> · :8080`; the sidebar's RUN group lists the commands. Ports load with `session.ports` per workspace on refresh
  and stay live through the `session.ports` event. **Run setup dialog:** command cards (Name,
  Folder, Command, Env file, Env as multi-line `KEY=VALUE`, and a `Close the pane when it ends`
  checkbox, off by default: on, an exit is typed after the command); only the card holding the
  keyboard shows its fields, every other one folds to a line (number, name, command, folder;
  Duplicate / Remove on hover) that opens on click (ADR 0063), `+ Add command`, `Duplicate` (a
  copy right below, named `<name> copy`, refused at 8), `Remove`, a
  `Start with the daemon` checkbox; Ctrl+Enter (or Enter outside Env) saves; errors show in
  the dialog. Folder and Env file have `Browse…`: a picker over the workspace (`fs.list`,
  ignored files shown) that opens folders on click and goes `↑ Up`; for Folder it starts at the
  workspace root and `Use this folder` (Enter) takes the shown one (`.` at the root); for Env
  file it starts in the card's folder (the root when that is not a folder of the workspace), can go up to
  the workspace root, and a click on a file takes it relative to the workspace (ADR 0049). Esc closes only the picker. Palette: `Run: Start all`
  (Ctrl+F5), `Run: Restart all` (Ctrl+Shift+F5), `Run: Stop all` (Ctrl+Alt+F5), `Run: Setup…`, `Run: Show Run tab`; the Run menu
  shows the three keys. Restart / Stop act only while a run session lives; Start with nothing set up opens Run setup.
- **Ports dialog** (spec 2026-11-01, ADR 0091): every listening TCP port of the daemon's machine, opened by `Ports: Show…` in the palette and `All ports…` at the end of the Run menu.
  One group per workspace with ports (sidebar order), then `OUTSIDE MKREW`; a row shows `:port`, `main` or `⎇ <branch>`, the run command (else the pane title, else the process name), `pid N` and a red ■.
  A click on a row with a pane opens that pane and closes the dialog.
  ■ (or Delete on the selected row) stops a run command at once; any other row asks in the footer ("Stop node.exe (pid 1234) on :3000? The pane stays open.") with Cancel and a red Stop that runs only after Tab moved onto it; Esc cancels the question, then closes the dialog.
  `port.list` is asked when it opens, every 2 s while it is open and after each Stop; nothing runs while it is closed; it closes on disconnect.
  `↑` / `↓` select, Enter opens the pane.
- **Empty states** everywhere (see §14).

### Limits chip (ADR 0106, replaces the widget bar of ADR 0102)

- A chip in the title bar's right group, before the Run pill, shown only while an agent's limit window is at 80 % or more (the rounded percent); no data, or nothing at 80 %, means no chip.
- It shows the highest window, `⚠ Claude 5h 84%`; a tie goes to the window that resets later.
  Amber (`status_needs_input`) from 80 % to 94 %, red (`danger_text`) from 95 %; always the ⚠ icon and the text.
- The tooltip lists every known window (Claude 5h and 7d, Codex 5h and 7d) with its percent and its reset in local time and as "in 2 h 10 min".
  A window whose reset time passed counts as unknown until a new report.
- A click opens Settings at the Integrations page.
- The chip is inert while a dialog is open and while the window is reconnecting.
- Every Space window has it; detached windows have none.
- Sources are unchanged: Codex from its transcript at a turn end (§20E.3), Claude from `mk statusline` (`limits.report`).
- `[ui] widget_bar` and `[ui] widgets_hidden` are retired keys (§20.0).

### Detached windows (ADR 0096)

- A terminal tab moves into a window of its own by dragging its chip outside every Mkrew window (the new window opens at the pointer), with `Shift+F4`, or from the tab menu and the palette.
- A detached window draws the same title bar (ADR 0106): its tabs as chips with the workspace name in front, Move all back, Split ▾ and the window controls; it has no workspace switcher, Board, Docs, Run pill or limits chip; it has the `+` row (a note goes to the window's workspace). Below it are the tab's panes.
- Tabs move between windows by dragging their chip onto another window (onto its strip: at the gap under the pointer) or with the tab menu's Move back / Move to window: `<tab> - <workspace>` (named by the tab that window shows).
  Move back and "Move all back" go to the Space window showing the tab's workspace; in a window per workspace, when no window shows it, a new one opens for it (ADR 0115).
- Its keys act on its own tab only: tab and pane commands of §13, the palette (its commands only: also Window: Close and Window: Move all tabs back), the context menu, the composer. The `/` and `@` pickers and the prompt picker open in the window of their composer. Turns… opens in the Space window used last. Settings (Ctrl+, or ⌘,) opens in the Space window used last on every OS (not over another dialog, not while reconnecting).
- Closing a detached window (✕ or Window: Close) closes its tabs (one confirmation for running agents); quitting Mkrew keeps them and reopens the windows on the next start (`<data>/windows.json`).
- A hidden or minimized detached window detaches its panes like a Space window.
- File and diff tabs tear off like terminal tabs (ADR 0108): a detached window draws file and diff panes from the workspace's store, one pane per file across all windows; opening a file that is open in another window focuses it there and raises that window.

### Window modes (ADR 0115)

- `[ui] window_mode` picks how workspaces map to windows: `single` (default) is one Space window with the sidebar listing every workspace; `per_workspace` gives each opened workspace its own Space window, at most one per workspace.
- Mode 1 looks and behaves as before.
- Mode 2 has no sidebar: `◧` is hidden and the sidebar commands (Toggle sidebar, Toggle compact sidebar, Collapse / Expand all workspaces, Focus sidebar filter) are not in the palette and do nothing.
  The tabs and the palette's Sessions scope list the sessions, the Run pill holds RUN, the "need you" chip and Ctrl+Shift+I replace the Inbox footer.
- A mode 2 window shows its workspace in the OS title (`<workspace> - Mkrew`, so the taskbar and Alt+Tab name it) and a 3 px stripe of the workspace color along the top (none without a color).
- The switcher lists every workspace; one that has a window ends with `◳`.
- **Mode 1 to mode 2:** the palette "Window: Open each workspace in its own window…", the Settings row Windows, or the same row of the Split ▾ menu (mode 1, two or more workspaces).
  The picker "Open workspaces in windows" lists every workspace with a checkbox (avatar, name, working and needs-you counts); the shown one is ticked.
  Typing filters; ↑ / ↓ move the highlight into the list, where Space ticks; Alt+A ticks every shown row (or unticks them all); a click ticks; Enter confirms; Esc cancels and nothing changes.
  The button says `Switch` with one ticked and `Open N windows` with more; with none it is disabled.
  With two or more, a confirmation step says which workspace stays in this window and which open in new ones, with [Back] and [Open N windows].
  The kept workspace is the shown one when ticked, else the first ticked; new windows open in list order at the asking window's size, each 32 px right and down from the previous, and the asking window keeps the keyboard.
  With zero or one workspace the mode changes at once.
- **Opening a workspace in mode 2** (the switcher, Alt+1…9, the palette, the end of Add workspace, and every jump to a session: Ctrl+Shift+I, a notification, the tray, the "need you" chip, the palette's Sessions scope): a workspace that has a window is focused (and the session with it); otherwise `[ui] open_workspace` decides.
  `new_window` opens one, `replace` shows it in the asking window (the old workspace's agents keep running), `ask` (default) asks "Open `<workspace>`" with [New window] (default) and [This window] and "Don't ask again", anchored under the switcher or centered for a key or the palette.
  Ctrl+click or Ctrl+Enter takes the other choice than the saved one (with `ask`: a new window without asking).
  A session that lives in a detached window comes forward there.
- **Mode 2 to mode 1:** the palette "Window: Combine into one window", the Settings row or the Split ▾ menu.
  The window the user is in becomes the single window with the sidebar and keeps its workspace; every other Space window closes without a question (sessions and tabs live in the daemon); detached windows stay; a toast says "Combined N windows" (N counts every window combined, this one too) when some closed.
- Changing `window_mode` in Settings runs these flows (`per_workspace` opens the picker and saves only when it is confirmed); a hand edit of `settings.toml` switches without a picker (to `per_workspace` every window becomes its workspace's window; to `single` they combine into the one used last).
- **"Need you" chip** (mode 2): while sessions of other workspaces need the user (§20J), the title bar shows `● N need you · <workspace>` (or `· M workspaces`) in the needs-input color.
  A click lists them, `<workspace> · <title> · <state>` with the workspace avatar, newest first; picking one shows the session where it lives.
  It never shrinks: the tab area gives way first.
- **Closing:** in mode 2 a Space window's ✕ closes that window only; the agents keep running and its detached windows stay.
  Closing the last Space window hides every window to the tray with `[ui] close_to_tray`, else Mkrew saves and quits; detached windows alone never keep the app running.
  A workspace that is removed or archived closes its window, except the last one, which shows the default workspace (the one with the newest live session), or the empty state with Add workspace when none is left.
  A palette or menu whose window closes closes with it; an open dialog moves to the Space window used last with what was typed.
- `[ui] panel_side` (`right` default, `left`; both modes) puts the Files / Session / Git panel on that side of the panes; its resize edge and the toggle icon follow; the palette "Panel: Move to the other side" switches it.
- `<data>/windows.json` version 2 keeps the Space windows (`spaces`: workspace, bounds, maximized, full screen; the one used last at the end, which gets the keyboard at start) and the detached windows.
  Mode 1 opens its one window where the window used last was, showing the workspace it showed before modes existed (the one with the newest live session); mode 2 restores every entry whose workspace is listed, the one used last in front.
  A version 1 file reads `main-window.json` once as the single Space window.

---

## §11 UI: per-session status

Status shows in two places: the tab status icon (and its tooltip) and the sidebar row.
A pane has no header and shows no status pill, elapsed time or context share (the composer shows the context share).
The tab icon shows only a pane that needs input (bell) or an agent that works (hourglass).
The tooltip gives the status in words, the time and the turns, so color is never the only signal.

| State | Icon | Extra |
| --- | --- | --- |
| starting | ◌ | |
| working | ● a still dot in the working color (no animation, ADR 0086) | elapsed time |
| needs_input | ? | `status-needs-input` stripe at the pane's top edge, counted in "Needs you" |
| idle | ○ | idle time |
| done | ✓ | counted in "Needs you" until viewed |
| exited | ✕ | exit code (History); its sidebar row and pane close (ADR 0033) |
| unknown | · | |

- Each state's mark has its own color (`status_working`, `status_needs_input`, `status_done`, `status_exited`, `status_idle`; dark and light), drawn beside its icon or text, never alone.
  Nothing loops while an agent works: the dot is still and the time text moves (ADR 0086).
  The loading mark (a pane being started, a transcription, a Jira fetch, the omp dock's working row, an omp `spinner` of style `dots`) is three dots taking turns on the status tick.
  Any other omp `spinner`, and a progress of unknown amount, is the orbit mark: a 12 px ring at 22 % with a 3 px dot that moves one of 8 places per tick (ADR 0116).
  A pane shows dots in one place only; the model pill, the todo HUD, checklist phases and subagent chips show still marks.
- Pane signals (ADR 0054, ADR 0106): a pane has no header.
  Transient signals are a small stack of floating badges in the pane's top-right corner, drawn only while they apply: a conflict warning, `⧗ N queued`, `Copy mode` with the key hints, and while the pane is zoomed `⤢ Pane zoomed` with a "Show all panes" button.
  Nothing is drawn in the normal case.
  The badges that only show text let clicks pass through to the terminal; the conflict warning's Dismiss and the "Show all panes" button take their own clicks.
  The agent controls (Terminal | Chat, Reading width, `Share`) are icon toggles in the title bar (§10), acting on the focused agent pane.
  `Share` (its menu has "Follow in Zed", ✓ while on) shows only with `[features] share`.
  "Turns…" and "Follow in Zed" are in the tab menu (right-click a tab).
  A pane is renamed with F2, the tab menu's "Rename pane" or a double-click on a tab that keeps its default name: the field shows in the tab chip (Enter saves, Esc cancels, blank clears; run panes keep their command name, ADR 0031).
  A tab still named `Terminal` / `Terminal N` shows its focused pane's session title (cut with `…`, full on hover) and follows the focus to another pane.
  A name the user gave the tab always wins.
  A double-click on a default-named tab renames the focused pane, on a named tab the tab.
  The focused pane decides the chip's title and icon for every kind of leaf (ADR 0108): a session as above; a file shows its file-type icon and file name, `●` while unsaved, and an italic title while it is the preview tab; the Diff pane shows the diff icon and `Diff`.
  Only a session pane can be renamed: F2 and a double-click on a tab whose focused pane is a file or the diff rename the tab.
  Without a title an agent pane shows the title its agent gave the terminal, e.g. Claude's conversation summary.
  Everywhere a session is named the order is: user title > terminal title > agent name.
  The active tab has a solid card fill (the Tern chip look); in a split the focused pane has a stripe
  at its top edge (§14), the unfocused ones are dimmed. The sidebar session row also puts the badge
  before the title. The badge is a bordered square chip (a logo chip has no border at rest, one
  color, ADR 0058); it also shows on board cards and in agent menus.
- **Chat mode** (ADR 0104): an agent pane whose profile has a readable transcript (Claude, Codex, omp) has a Terminal | Chat toggle in the title bar and `Ctrl+Alt+T` toggles it (palette: "Agent: Toggle chat view").
  The mode is kept per pane in the tab layout: the pane's leaf holds `chat: true` or `chat: false` (it survives a UI restart and the daemon's startup resume), and a leaf without the flag follows `[ui] chat_default` (default `false`).
  Picking Terminal or Chat (the title bar toggle, `Ctrl+Alt+T`, the palette) also writes `[ui] chat_default` (the last pick, shown in Settings as "Chat view by default") and pins the panes that had no saved mode, so new agent panes and the next start use it (ADR 0105).
  Chat draws the last 50 turns of the transcript: the prompt as a right-aligned card, the agent's text as Markdown (selectable inside one message), tool calls as collapsed rows `name target` that open to the arguments and the result (an error result in the danger color), thinking as a collapsed row (Claude, omp), the latest plan as a card, a pending Claude question as a card whose "Answer in terminal" button switches to Terminal, and a tray of the turn's running Claude subagents.
  The plan comes from Claude `TodoWrite`, Claude's task tools (`TaskCreate`, `TaskUpdate`, `TaskList`, which current Claude Code uses by default) or Codex `update_plan`.
  A Claude subagent started in the background stays in the tray until its `<task-notification>` arrives.
  Codex tool output is read in the shapes current Codex writes: a JSON string with `metadata.exit_code`, or text with an `Exit code: N` or `Process exited with code N` line before `Output:`.
  The tool and thinking row toggles, the "earlier turns" link and "Answer in terminal" are Tab stops (Enter or Space presses them); PageUp, PageDown, Home, End and the arrows scroll the list, and Esc ends a selection, else returns to the composer.
  ADR 0105 gives the page a centered column of 760 px with `space::XL` side padding: the prompt is a right-aligned bubble, the agent's text has accent headings and bullet dots, inline code and file paths are pills (a path in a Chat message opens the file in a file pane at its `:line`), code blocks are cards with a language label and a Copy button (```` ```mermaid ```` blocks are diagrams with Expand, ```` ```csv ```` and ```` ```tsv ```` blocks are tables, §12), and thinking and tool calls between two texts are one activity row ("Thought for 4s · 3 commands, 2 reads", "Working…" while it runs) that opens to the thinking text and the tool rows.
  An omp turn ends with omp's recap line when it is not older than the turn's prompt and the agent is not working.
  The composer is a floating card in the same column (§20B).
  In a narrow card Send is never clipped: the chip cell shrinks and clips, and the Prompts and sparkle tools drop whole when the row has no room for them (ADR 0107).
  The terminal keeps running underneath and still reports the state.
- **omp native surface** (ADR 0107): an omp pane whose session has `tsp` `live` draws omp's own interface instead of the terminal grid or Chat, and has no `Terminal | Chat` toggle: the title bar (§10) shows none while that pane has the focus, and `Ctrl+Alt+T` and the palette entry do nothing for it.
  omp describes its interface as a tree of nodes (the Tern Surface Protocol, sent inside the PTY stream); the daemon applies the tree and the UI draws it in Mkrew's colors, spacing and fonts (§14), dark and light.
  `main` is a scrolling list that follows its end, with the scroll-to-end button, laid out only around what is on screen (ADR 0114): in the Chat page's centered column (`CHAT_COLUMN_WIDTH`) when the pane is the only one shown in its tab (or zoomed), and as wide as the pane less `space::XL` at both sides, like Tern, when other panes are shown beside it (ADR 0112; the PTY is sized to the same width); `dock` (the HUD pills, the status, the context meter, the editor, and the bar with model, effort, rate, cost and Send or Stop) sits under it in the Chat column and is always shown; `layer` children and `overlay` nodes are a modal inside the pane; a `screen` surface covers the whole pane until omp closes it.
  Keys, paste, image paste and file drops go to the PTY as in a terminal pane (Mkrew's own shortcuts first), except in the composer field, which Mkrew edits itself while omp's focus is on it, no modal is open and the editor is not in vim mode (ADR 0113): typing, deletions, caret moves, Shift selection, Ctrl+A, Ctrl+Z / Ctrl+Y, Ctrl+C / Ctrl+X with a selection, Ctrl+V with text and Shift+Enter change the field at once (a visible caret, the selection highlighted, a click or drag places the caret) and go to omp as one `edit` event; Enter, Tab, Esc, the arrows in omp's completion list, Up on the first line and Down on the last (history), Ctrl+C / Ctrl+X without a selection, an image paste and omp's other shortcuts go to the PTY after the caret.
  A text omp draws that Mkrew did not send (a completion, the history, a send that empties it) replaces the field as one undo step.
  The editor is one wrapping text, so a skill or mention stays inline with the prompt; an attachment is a small accent `#n` chip whose click shows its image or text in a panel above it.
  A click on a node with a click action (the model chip, Stop, Send, the agent pill, the jobs pill, the context meter, the cost) goes to omp, which opens its own selector or overlay; the events sent are `action`, `toggle`, `select`, `activate`, `focus`, `theme` and `edit`.
  The context meter's row shows the conversation's title (the session's name) before the window size; a space of `space::L` parts the last message from the dock, and `space::S` parts the status row, the meter and the card.
  A pane that shares the tab's height with another shows only the composer card's field while it has no keyboard; the status row, the meter, the chips and the bar come back when it has.
  Text selects inside one node and Ctrl+C copies it; a node's `title` is its tooltip; hovercards are ignored.
  An older document is cut when it passes 20,000 nodes or 1 MiB of text (counted as JSON, so an escape such as `\u001b` counts 6 bytes): the oldest rows of `main` go and the list starts with "Earlier messages were trimmed".
  A document that is still over a bound with `main` empty (the excess is in `dock` or `layer`) makes the session fall back ("document over bounds").
  At most 3 surfaces stay open: a fourth open drops the oldest read-only one, else the oldest.
  The bounds keep a `tsp.snapshot` reply under the 16 MiB frame: 3 x (1 MiB of text + 20,000 nodes at about 100 bytes) + 5.3 MiB of base64 blobs is about 14 MiB.
  The pane by `Session.tsp`: `off` is the terminal pane with the title bar's `Terminal | Chat` toggle; `waiting` (TSP on, no surface yet) is the starting state of an agent pane; `live` is the surface; `fallback` (no hello within 10 s of the session's first output, because resumed and concurrently started omp sessions need over 2 s to send it, or an unsupported hello version, or omp closed the surface with `x {keep:false}`, or omp said hello but opened no surface within 60 s of the first output) is the terminal grid without the Chat toggle and one info toast "omp is drawing as text: <reason>"; a hello that still arrives within 60 s of the first output after the 10 s fallback is answered and the pane switches to the native surface, while later hellos and the other fallbacks are final; an exited session keeps its last document, read-only, under the existing exited notice.
  While `live` the state comes from omp's hooks only (§6); without hooks it is `unknown`.
  `mk web` shows "This omp session uses Mkrew's native view. Open it on the desktop." in place of the terminal of a live session that has not exited; an exited one shows its replay, which holds only what omp printed before the hello (the log keeps plain bytes only), so the read-only document survives only in a UI that was connected when omp exited; its state, Stop, the reply chips and the quick keys keep working.
  The dock is drawn as Tern's composer from the pieces of the Chat composer card (§20B, ADR 0105, ADR 0116), with omp as the only source of the content: the context meter above the card (a 1 px line, `text_muted` fill, `status_needs_input` past `thresholds.warn`, `danger_text` past `bad`, the percent above the end of the fill and the total at the right, marks as ticks shown while the meter is hovered, a click sends `status.context`), the card (`surface_raised`, `radius::COMPOSER`, glass border, float shadow) holding the editor line (the placeholder in `text_subtle`, upright, no icon; omp's `ghost` text right after the caret) and the chip row: the model pill (a 1 px `border` ring with no fill, `text` at 500, chevron; omp's effort glyph leads it and the effort label is in its tooltip), quiet mono fact chips in `text_subtle` (colors omp puts in spans stay; an `accent` span such as `⚡ yolo` takes the Chat permission chip look), the rate and cost chips, and the round Send (`CornerDownLeft`, a neutral fill that turns to `text` on hover, never accent) or Stop (a `text` circle with a square) at the end of the row.
  Above the meter, the status row has the working row (loading dots, the elapsed time in mono `text_subtle`, a divider, the label in `text_muted`; the card's Stop is the only Stop), the todo HUD, and the hud as one pill with a `border` ring and no fill whose segments (the agent and jobs pills, each an icon and a count in mono) are split by dividers.
  When the chip row is too narrow it drops, in this order: the facts by omp's `priority` (highest number first), the cost, and last the model name truncates (not below about six characters); the model pill never shrinks and Send or Stop is never clipped.
  A dock without omp 18.13.2's roles (`omp.editor`, `omp.composer.*`, `omp.hud*`, `omp.working*`) is drawn by the generic fallback.
  Nerd Font private-use glyphs in text (spans styled `icon`) are drawn upright in the terminal font with `TERMINAL_GLYPH_FALLBACKS`, whose first entry (Symbols Nerd Font Mono) ships with the app (ADR 0116).
  A question from omp (`ask`) replaces the dock's editor and is drawn as the composer card: tab chips in its top row (answered tabs start with a check, a last Review tab, the active tab on a solid `card` background), the question in `weight::SEMIBOLD`, then one row per option with a radio circle (a check box for multi-select) filled with `accent` for the recommended one, the label, the detail muted below, a "Recommended" badge on the right and the picked row on `surface_raised` with a 2 px `accent` bar at its left; "Other..." is a row of the same list; an optional Markdown preview sits under the list.
  The Review tab shows a notice line and the table of answers.
  The bottom row has pill buttons (Hide, Note, Skip, Next or Submit; the default one in `accent` with `on_accent` text; the key hint is the tooltip) and, when the question has a timeout, a small ring with the seconds left.
  Hide folds the card to one row ("Question ...", Skip, Show).
  A click sends `select`, a double-click `activate`, a button `action`; keys still go to omp, and "Other..." opens omp's own text field.
  omp's todo (`checklist`) shows as a one-line HUD at the right of the status row above the dock (the active todo mark and the active item, else the first pending one, else "All done", then `done/total` in the terminal font, muted); a click opens the phase list in a glass panel above the row (at most 60% of the pane height, scrolling; Esc or a click outside closes it, nothing is sent to omp).
  The same phase list is the body of the todo card in the conversation (`full`) and of the todo reminder (a muted note, done phases collapsed): a header per phase with a status mark (the active todo mark while it has an active item, a `status_done` filled circle when all items are done or dropped, a muted ring otherwise), its Roman numeral, title (`text`, `text_muted` once done) and `done/count`; a click on a header folds it; items are marked active (`accent` ring and text), pending (muted ring), done (`status_done` check, muted text), dropped (muted, struck through) and blocked (`status_needs_input`, " (blocked)"), with a note as a muted second line.
  A thinking head (`omp.thinking`) is META italic `text_subtle` with its chevron after the label.
  Top-level rows that show only one collapsed thinking section, with only hidden rows between them, show as one head: "Thought for <sum of `took`>" (whole seconds, at least 1 s), or "Thought N times" when one has no `took`; a click opens the run, a click on its head closes it (ADR 0116).
- Pane menu (right-click a sidebar session row, or Shift+F10 with a terminal focused; a pane has no header to right-click): **Pin**, **Mark unread** (a finished agent goes back to Needs you until its pane is focused again, §20J; greyed otherwise), **Rename** (inline
  like F2; greyed for a run pane) and **Change color** (the workspace palette; none clears).
  The tab menu has Rename pane, Close pane, Zoom pane, Follow in Zed, Turns… and Change color for the focused pane.
  The palette has "Pane: Pin / Unpin" and "Agent: Mark unread".
  A colored pane draws a 2 px line of its color at its top; the border still shows state (ADR 0037).
- Tab icon: a bell while any pane needs input, else an hourglass while an agent works, else none (§10, Terminals).
- Workspace row: `● N working` (a pill, a still dot) while an agent works, else `N idle`; amber
  `? N` while N need input; no token total (the Session tab shows usage). Sidebar session rows
  sit indented in the workspace's card (ADR 0062): agent chip (an accent chip while working;
  its hover gives the
  state in words and when it changed), title, the state glyph of a waiting or done agent (none
  while idle) and, on the right, only the time of a working or idle session (`17m`). Agent rows,
  selected or not, show just the session/task title on one line (the chip names the agent);
  terminal rows keep `powershell · title`. Agent terminal
  titles drop omp's leading `π` logo (ADR 0031).
- Pinned workspaces, sessions and tabs lead their lists and show 📌 next to the name (§10 Pin);
  the order does not change a pane's status.
- Hovering a status shows its source (hook/screen) and when it last changed.

---

## §12 UI: Markdown review (inspired by Antigravity)

Read plans/specs written by agents, comment on passages, then send the comments back to the
agent session that wrote them so it edits the file **in the same conversation context**.
Off unless `[features] docs = true` (§20.0, ADR 0046): no Docs rail tab, command or "Review in Docs".

- **File list:** files matching the workspace's `review_globs` (default
  `["*.md", "docs/**/*.md", ".claude/plans/**/*.md"]`) plus `.md` files recently changed by agents.
  Show open-comment counts. A filter field at the top (`Ctrl+Shift+F`) fuzzy-filters the list by
  path; ↑ / ↓ in it show the previous / next file.
- **Rendering:** `pulldown-cmark` → GPUI elements: headings, paragraphs, lists, task lists, tables,
  code blocks, links, quotes. Toggle to source view with line numbers. Code blocks are
  highlighted by their fence language (syntect, as in the Editor); the text uses the doc
  typography of §14 (ADR 0021). ```` ```mermaid ```` blocks are drawn as diagrams (pure Rust
  renderer, theme colors); a block that cannot be drawn shows its source and the reason.
  ```` ```csv ```` and ```` ```tsv ```` blocks are drawn as tables on the code card (label `csv · N rows`, Copy copies the source; ADR 0021 addendum).
  The first record is the head; quoted fields may hold separators, quotes (`""`) and line breaks; short records are padded; a column whose body cells are all numbers is right-aligned; each column's width follows its longest text (4 to 40 characters).
  At most 200 rows are drawn, then "N more rows"; a block that cannot be read (an unclosed quote, no rows) shows its source and the reason.
  Diagrams and tables are drawn wherever this renderer is used: Docs, the Files preview, Chat messages (§11) and omp's native surface.
- **Comments:** pick lines (click a preview block or a source line, Shift+click extends; Shift+↓ / Shift+↑ pick the first line or block in view, then grow or shrink the pick by one line or block, scrolled into view; Esc clears it; a text field keeps its own Shift+↑/↓, ADR 0085) →
  "Add comment". Anchored by line range + quoted text (the picked source lines). Threads with
  replies, resolve, reopen. Listed in the right margin in anchor order; clicking a thread's
  lines highlights them (ADR 0006; character selection in Docs and anchor-aligned cards are
  backlog; the Editor's preview selects characters, ADR 0072).
- **Re-anchoring:** on `doc.changed`, find `quote` in the new content to update lines; if not found
  → `outdated` (still visible, never auto-deleted).
- **Send to agent:** sends all open comments of the file (or the selected ones) to one session.
  Default target = the session that most recently modified the file (attributed by the file watcher:
  sessions running in that project when the file changed); otherwise the user picks.
- **Delivery:** the daemon composes a prompt and types it into the session PTY (queued while the
  session is working). Template:

```
Update docs/plan.md according to the review comments below.
Only edit this file. When done, for EACH comment run:
  mk comment reply <id> "<what you changed, or why you did not>"

[c_01JX] lines 12–14, passage "Use Redis for caching":
  Why not an in-memory cache first? Is Redis overkill?
[c_01JY] line 40, passage "M3: deploy":
  Add a rollback step.
```

- **Agent replies:** `mk comment reply` from inside a Mkrew PTY adds a reply authored by the agent
  and moves the comment to `addressed`. The user reviews, then resolves or replies and resends.
- **See changes:** after a send, "View changes" diffs the snapshot at send time vs now (`similar`).

---

## §12A UI: Files (tree and file pane)

- **Tree** (the right panel's Files tab, ADR 0081; opens with `Ctrl+Shift+3`; its rows drag onto
  panes, §20B.5): lazily loaded per folder, respects `.gitignore` (an
  eye toggle beside the search shows ignored files; accent while they show, ADR 0110),
  git status markers next to names (M modified, A added, D deleted, ? untracked), a small dot on
  folders containing changes. In a workspace with child repositories (ADR 0029) the markers come
  from the project folder's repository and from each child repository the listed folder is in
  (all of them when the root is listed). Updates live when agents create/delete/rename files.
- **File pane** (ADR 0108, replaces the Editor view): an open file is a leaf of a terminal tab's split tree.
  It is a tab in the title bar's tab area (file-type icon, file name, `●` while unsaved, × to close) or a pane inside the tab in front, inside the same frame, stripe and dim as a terminal pane (§14).
  There is no Editor view, no Editor chip and no file tab strip.
  A workspace has at most one pane per file across all its tabs and windows: opening a file that is open focuses its pane (its tab and window come to the front) and jumps to the line.
  A single click on a file in the tree, or Enter on it, opens the workspace's **preview tab** (italic title), which the next single click replaces while it is clean; a double click, Alt+Enter, an edit, Pin or a drag (a move of the pane) keeps it.
  A tree click keeps the keyboard in the tree.
  Every other open (palette, a terminal link, a Markdown link, a chat path pill, "New file…", Search hits) opens a kept tab right after the active tab.
  Dragging a tree row onto a pane opens the file as a split at that edge (§10 "Drops").
  The open files are saved in the tab layout (`{"file": "src/a.rs"}`, plus `task`, `md` and `preview`) and come back after a restart in the same place; their text is read when the pane is first drawn.
  A file is of one folder for good: the project's, or a task worktree's (`task`). Its read, its Reload and its Save use that folder, whatever the Files tree shows; a tree click, a drag and a note take the folder the tree shows at that moment (a new note: the project's), and a terminal link or a chat path pill takes the worktree of its agent. After a restart or a lost connection the file still reads and saves there; when that worktree is gone the pane says it cannot read the file. Choosing another worktree in the tree changes only the tree.
  A file deleted on disk shows "This file was removed." with a Close button.
- **Editor** (the text editor of a file pane; ADR 0028): the opened file with line numbers and syntax highlighting via
  `syntect` (lazy-loaded syntax set; re-highlighted 150 ms after typing stops). A plain editor:
  click places the caret, Shift+click / drag select; arrows (Ctrl = by word), Home/End
  (Ctrl = file), PgUp/PgDn, Backspace/Delete (Ctrl = word), Enter keeps the indent, Tab, Ctrl+A,
  Ctrl+Z / Ctrl+Y undo/redo, Ctrl+C/X/V. Line endings and the final newline are kept. Unsaved
  changes show `●` before the path; Ctrl+S or "Save" writes the file (`fs.write`). Files > 2 MB
  or binary: a read-only notice and "Open in Zed". A scroll bar on the right shows where the
  view is (only when the file is taller than the view): drag its thumb, or press the track to
  jump there.
- **Closing and tabs** (ADR 0108, replaces the file tab strip of ADR 0080): a file's tab is a tab of the title bar (§10), so pinning, reordering, Ctrl+Tab, Ctrl+Alt+1-9, Rename, Close, Close others, Close to the right and the tab menu work as for terminal tabs.
  A file is closed with the tab's ×, the tab menu's Close pane, `Ctrl+W` or `Ctrl+Shift+X` (both close the focused file pane) or a middle click on its tab; each asks "Save / Don't save / Cancel" first for a file with unsaved text (Cancel closes nothing).
  The window's find bar belongs to the shown file pane: one find bar per file pane, searched on that file.
  `git.changed` reads the files of the shown panes; files of tabs that are not shown are read when shown.
  The worktree of every open file pane stays watched, in a shown tab or not and in the shown workspace or not, until its last file closes (unless the Files or Git view shows that worktree).
  A file of a task worktree shows the task's title in muted text after its path in the pane header, in the tab chip's tooltip and in the unsaved-changes question; the tab chip's label stays the file name.
  Moving a file (drag to an edge, to another tab, tear-off, merge) never asks.
- **Change gutter** (ADR 0047): right of each line number a thin bar marks how the line differs
  from the file's text in the git index (what the Git tab calls unstaged): added
  (`diff-add-text`), changed (`diff-mod-text`), and a short `diff-del-text` mark at the top of
  the line after removed lines. It follows unsaved typing (recomputed with the highlight, 150 ms
  after typing stops; marks on lines edited since are hidden until then) and staging/committing
  (the `git.changed` re-read). Untracked files, files outside a repository and conflicted files
  have no bars; the bar column is always there so the text never moves.
- Unsaved text is never replaced (§2.5). If the file changes on disk meanwhile (an agent), or
  Save finds another version on disk, a banner "Changed on disk: Reload · Keep mine" offers
  "Reload" (drop your changes) or "Keep mine" (Save overwrites it). Closing a pane, a tab, a window or a workspace, or quitting with unsaved changes asks "Save / Don't save /
  Cancel" (once per unsaved file). Switching workspace or tab keeps them; a preview tab with unsaved edits is kept, not replaced.
- **Markdown preview:** a `.md` file opens as a rendered preview, by the Docs renderer (§12,
  §14: the same typography tokens for light and dark, highlighted code blocks, ```` ```mermaid ````
  blocks drawn as diagrams, built off the UI thread). The header has a `Preview | Split | Source`
  switch (ADR 0083, ADR 0092); Source is the editor above. Ctrl+Shift+V toggles Preview ↔ Source
  (Split goes to Preview), Ctrl+Alt+V toggles Split ↔ Source. The mode a file is switched to is
  the mode the next `.md` file opens in (`[ui] md_mode`, default `preview`). In Preview (ADR 0072) the text can be selected with the
  mouse across blocks (Shift+click extends, double-click takes a word, Ctrl+A selects the page,
  Esc clears) and Ctrl+C copies it as shown (blocks and lines on their own lines, table cells
  tab-separated, list markers `- ` / `N. ` / `[x] `, drawn diagrams copy nothing; a drag or
  double-click adds no marker to the list item it starts in, items after it keep theirs, and
  Ctrl+A copies every item with its marker). Hovering a top-level block shows ✎ at its top-right corner: its Markdown opens in a
  field in its place; Ctrl+Enter puts it back over the block's source lines (one undo step,
  unsaved like any edit), Esc or a click outside cancels; refused with a toast when the text
  changed since the field opened. The preview is built from
  the editor's text, so unsaved changes show in it and switching never loses them; it is parsed
  when the file is read, when switching to Preview or Split after an edit, and in Split after
  every edit (one build at a time per file; an edit made meanwhile is built once it lands). Find
  and "Open at a line" switch Preview to Source and keep Split. A click on a task box ticks or
  unticks it in the source (one undo step, unsaved; refused with a toast while the page is of an
  older text). The preview's links are clickable (Editor and Docs, ADR 0092): `http` and `https`
  open in a browser pane (§20M), `mailto` opens in the mail app, `#heading` scrolls to that heading (GitHub anchors), a
  relative path opens that file of the folder in a file pane (`/path` from the folder root,
  `#L12` at a line); other schemes and paths leaving the folder open nothing (a toast says so).
  GitHub alerts (`> [!NOTE]` / `TIP` / `IMPORTANT` / `WARNING` / `CAUTION`) show a colored bar
  with an icon and title; YAML front matter at the top shows as a bordered card of `key value`
  rows; code blocks show "Copy" on hover. `.md` files also offer "Review in Docs". The page is a virtual
  list (only the blocks on screen are laid out; every block's height is measured once per page)
  with the editor's scroll bar, so long files scroll smoothly; Docs shows its preview the same
  way. A re-read or edited page keeps the scroll position; another file pane (or the same file
  opened again) starts at the top, and a live font change measures the page again.
- **Markdown Split view** (ADR 0092): the editor on the left, the live page on the right, a
  divider between them. Dragging the divider resizes both sides (each keeps 20–80 %); a
  double-click puts it back at half and half; the source's share is saved as `[ui] md_split`
  (percent, default 50) when the drag ends. The page follows the editor's scroll (the block at
  the editor's top line goes to the page's top); scrolling the page moves nothing else. A press
  on the page puts the editor's caret on that block's first line (scrolled into view only when
  it is off screen). The page has its own keyboard focus (selection, Ctrl+C, Ctrl+A); ✎ works as in Preview.
- **Send selection from the preview** (ADR 0103): `Ctrl+Shift+L` with text selected on the page (Preview, or Split while the editor selects nothing) sends the Markdown source of the blocks the selection touches, with `path:a-b` (§20B.4).
  A selection inside one block sends that block's whole line range.
  With nothing selected on the page it sends the editor's selected lines or its caret's line, as before.
- **Palette Files scope** (`Ctrl+Shift+O`, or the "Search files…" entry at the top of the tree) fuzzy-finds files of the current workspace, in the folder the tree shows (project or task worktree) (`ignore` to walk, `nucleo-matcher` to rank).
- **Palette Search scope** (`Ctrl+Alt+Shift+F`) finds text in those files.
- "Open in Zed" opens the file at the caret's line.
- **Right-click menu** on tree rows (files and folders) and on the blank part of the tree (the
  folder the tree shows: the project, or the task worktree): "New file…" and "New folder…" (a
  name field over the tree; Enter creates it in the folder clicked, or next to the file
  clicked; `/` in the name makes subfolders; a new file opens, a new folder opens in the tree),
  "Copy path" (absolute), "Add to .gitignore" (rows only: appends `/path` or `/path/` to the
  shown folder's `.gitignore`, once), "Open in File Explorer" ("Reveal in Finder" on macOS) and "Open in Zed" (§10). The app
  does these on the local disk; the watcher's `git.changed` lists the tree again. Failures show
  in the sidebar. A click outside or Esc closes the menu.
  Shift+F10 (⌃⌘M on macOS) opens it on the cursor row (on the active tab's menu while a file pane has the keyboard).
- **Open at a line:** Ctrl+Click on a file path in a terminal (§10) opens that file in a file pane (a new tab, or the pane that already shows it) with the
  caret on the line, scrolled into view. A path in another task's worktree switches to
  that worktree (asking about unsaved text first).
- **Find & replace** (VS Code style): `Ctrl+F` (also `Ctrl+Shift+F` or "Find" in the header)
  opens the bar, `Ctrl+H` opens it with the replace row. A selection on one line becomes the
  query. Case-insensitive; matches highlighted, Enter / Shift+Enter move between them and select
  the match, Tab / Shift+Tab switch find ↔ replace. In the replace field Enter replaces the
  current match and goes to the next; Ctrl+Alt+Enter (⌥⌘↩ on macOS; in either field) or "Replace all" replaces
  every match as one undo step. Replacing does not save. Esc closes the bar and returns to the
  editor.
- **New note** (ADR 0108): the `+` row's note button, `Ctrl+Alt+N` (⌥⌘N) and the palette's "Tabs: New note" create a Markdown file in the notes folder and open it in a new tab, kept, in source mode, with the caret in the editor.
  The daemon makes it (`fs.new_note`, §7): the folder is `[notes] folder` of the daemon's own `settings.toml` (§20.0; empty = the workspace root, relative = under the root, absolute = as is), the name is `note-YYYYMMDD-HHMMSS.md` in local time, `-2`, `-3`, ... is added while the name exists, missing folders are created, the file starts empty.
  A note inside the root is stored in the layout as a root-relative path, a note in an absolute folder as its absolute path.
  Dragging the note button onto a pane creates the note and opens it at that edge.
  An error (no write access, disk full, a folder that cannot be made) shows a toast with the reason and opens nothing.
  If the notes folder setting changes while a note from the old folder is open, the pane shows the read error with its reason and Close.

## §12B UI: Git (right panel tab and Diff pane)

The Git tab of the right panel holds, top to bottom (ADR 0110): the Working / Vs. base / History tabs (underlined text tabs under the repository tabs), the filter row (field, `Viewed N/M`, "Changed by"), the list, the commit box and a footer with the branch, its upstream, Fetch and Push; as wide as the panel (240-560 px; the filter row wraps).
Group headers are sentence case with a muted count (`Staged 0`); the empty Staged hint is a muted line; a file row is 28 px with its change letter colored and unfilled.
The Diff pane (ADR 0108) holds the toolbar (file, Unified / Split, Wrap, Viewed, Open in Zed, Discard, Stage file, `…`, ✕), unified / split, hunks, the comment margin and Send to agent.
It is a leaf of a tab's split tree like a file: a tab of its own in the title bar (diff icon, `Diff`) or a pane next to terminals and files.
A workspace has at most one Diff pane, across all its tabs and windows; it shows the diff of the Git list's selected file.
A click or Enter on a file of the Git list opens the Diff pane after the active tab, or focuses it where it is open; ↑ / ↓ in the list move it; `Ctrl+Shift+7` and the palette's "View: Diff" open or focus it.
The pane's ✕ or the tab's × closes it; `Esc` in the list unselects the picked lines and does not close it.
With nothing changed the pane shows the Diff empty state and stays.
A Diff pane in a detached window and one in a Space window never share scroll or layout state.

All git actions are user-initiated; Mkrew never commits or switches branches by itself.
The daemon shells out to `git` (porcelain v2, `-z` output) behind a `GitBackend` trait, so a
`gix`-based backend can replace it later without touching callers. Process spawning is slow on
Windows (tens of ms, worse with antivirus), so:

- One refresh = ONE `git status --porcelain=v2 --branch -z` call, run in parallel with the base
  lookup and (Working mode) the staged/unstaged `git diff --numstat` for the +/− counts
  (ADR 0007); Against-base mode adds `merge-base` (cached until HEAD or base moves) and one
  numstat diff since it.
- Refresh is triggered by the file watcher (debounced 300 ms, §7 `git.changed`) and when its tab or a Diff pane comes on screen; never polled.
- Diffs are computed per file on demand and cached by (path, mode, mtime/size, HEAD).
- Never run git for workspaces whose Git tab or Diff pane is not on screen, except the cheap status used for
  sidebar markers (at most once per 2 s per workspace).

- **Header:** with two or more repositories (`git.repos`, ADR 0029) a "Repo" chip row (the
  workspace name for the project folder, then the child folders; hidden while a task worktree is
  shown); then current branch, upstream, ahead/behind, and the **base** used for comparison.
  Base auto-detects `origin/HEAD` → `main` → `master`; configurable per workspace (menu or
  `mk project update <p> --base develop`). When the project folder is no repository, the first
  child repository is shown.
- **Two modes:**
    - **Working changes:** uncommitted changes grouped Staged / Unstaged / Untracked.
    - **Against base:** everything on this branch vs base, from `git merge-base HEAD <base>` to the
      working tree — like a pull request diff.
- **Changed-file list** (the Git tab) with +/− counts; selecting one shows its diff.
- **Diff viewer:** unified or split (side-by-side), hunk navigation, collapsed unchanged regions, syntax
  highlighting. Diffs > 1 MB show a summary and "Open in Zed".
- **Diff modes:** a Unified | Split toggle in the diff header (key `d`, §13) switches between one
  column (removed lines, then added lines) and two columns (old on the left, new on the right). Split
  is one virtualized list of paired rows, so one scroll handle moves both sides. In a hunk the i-th
  removed line is paired with the i-th added line and the shorter side is padded with an empty cell;
  unchanged lines show on both sides; gap and hunk-header rows span both columns. Paired rows (a line
  changed in place) use the *modified* colors, an unpaired removed / added line the delete / add
  colors; Unified shows the same pairs in the modified colors, keeping `−` then `+` signs. Hunk navigation (`n` / `p`) and
  diff comments work in both modes (comments pick new-side lines: the right column in Split). The
  mode is global (every file, every repo of the workspace, History too) and is remembered as
  `[ui] diff_mode` (§20.0).
- **Viewed files** (ADR 0065): each file row has a box (not in the History), the diff toolbar a
  "Viewed" button, key `v` (§13) and the palette "Git: Mark file as viewed / not viewed". A viewed
  row is dimmed and the selected viewed file's diff collapses to "Marked as viewed." with a
  **Show diff** button. The Git tab's filter row shows `Viewed N/M` (M = every file of the shown
  list, whatever "Changed by" and the file filter hide). The daemon keeps each mark in SQLite
  with a hash of the file's diff text; it clears by itself as soon as that text changes or the
  file stops changing (checked with every status refresh). Staging a whole viewed file keeps the
  mark; marks of Against base, a task's changes and Working changes are separate lists.
- **Stage / unstage** per file or per hunk (daemon builds the patch, runs `git apply --cached`).
- **Review flow** (ADR 0103): `v` on the selected file that is not viewed marks it and selects the next file below it that is not viewed, wrapping to the top (every file viewed: the selection stays); `v` on a viewed file only clears the mark.
  `n` past the last hunk of a file selects the next file that is not viewed and its first hunk, `p` before the first hunk the previous such file and its last hunk; there is no wrap and a file without hunks is passed on the next key.
  On a viewed file's folded diff, whose hunks are hidden, `n` / `p` go straight to the next / previous file that is not viewed; `x` there still discards the file.
- **Discard** per file or per hunk: always confirms and shows what will be lost.
  A hunk of a modified file in the Changes group has a "Discard hunk" button after "Stage hunk"; `x` with a current hunk opens the same confirmation for that hunk, else the file's.
  The daemon reverses the hunk in the working tree only (`git apply -R`); the dialog stays disabled and confirm refuses if a reload changed the hunk, and a hunk that still fails to apply shows git's text and reloads the diff.
- **Commit:** multi-line message box, Commit button (`Ctrl+Enter`), and ✦: the workspace's
  default agent writes a Conventional Commits message from the staged changes (else the
  unstaged ones) via `ai.generate` (§7); the answer replaces the draft, nothing is committed
  until the user commits. No amend in v1. With nothing staged but changes in the list, the button
  reads "Stage all & commit" (one `git.stage` of every unstaged and untracked file, then the
  commit); Ctrl+Enter first says so under the box and does it on the next Ctrl+Enter (an edit
  withdraws the offer).
- **Fetch / Push:** fetch refreshes the base; push sends the current branch to its upstream only,
  never with force. Pull, rebase, merge and branch switching are backlog.
- **History:** last 50 commits (`git log --parents`, which rewrites parents past commits a subfolder project hides;
  re-sorted so children come before parents, git's order is kept when it already is); select one to see its files and
  diffs. A lane graph left of each commit draws its dot and the lines to its children and
  parents (merges fork into a lane that ends at the fork point; lanes to commits past the 50
  run off the bottom), at most 8 lanes, colored from the workspace palette (no red or yellow);
  the expanded commit's files show its lanes running straight through.
- **Task changes:** from a Review-column task card, "View changes" opens the Git tab and the diff from
  `task.checkpoint` to now (ties into M5).
- **Diff comments → agent:** pick diff lines (click a new-side line, Shift+click extends; Shift+↓ / Shift+↑ pick the first new-side line of the current hunk (`n` / `p` choose it), then grow or shrink the pick one line, across hunks, scrolled into view; as in
  ADR 0006) → comment, reusing the M6 comment system (`context = 'diff'`, anchored to new-side
  lines + quote = the picked lines shown; not in History). Threads are listed in a margin beside
  the diff while the file has any (or lines are picked, a draft is open, or listing failed). "Send to agent" delivers them to the
  task's session (or a chosen one); the agent fixes the code and answers via `mk comment reply`.
  The prompt is the §12 template with `Fix the code in <file>` instead of `Update <file>`,
  `Line numbers are of the current file.` instead of `Only edit this file.`, and
  `code "<quote>"` instead of `passage "<quote>"`. Comments on non-Markdown files are not
  re-anchored when the code changes (backlog); `doc.list` ignores diff comments.
- **Extra tokens:** `diff-add-bg`, `diff-add-text`, `diff-del-bg`, `diff-del-text`,
  `diff-mod-bg`, `diff-mod-text` (a line changed in place, Split), `diff-hunk-header` (dark + light)
  in `theme.rs`. Diff backgrounds stay very subtle.

---

## §13 UI: keyboard shortcuts

All shortcuts live in `<config>/mkrew/keymap.toml`, which the Settings dialog's Keyboard page writes
(applied at once, ADR 0073); below are defaults. While a terminal has focus,
only combinations bound in the keymap are intercepted; everything else goes to the agent. Defaults
therefore avoid plain `Ctrl+<letter>` and favor `Ctrl+Shift`/`Alt`, like Windows Terminal.
Editor-only commands may use plain `Ctrl+<letter>` (ADR 0028): the Editor context is set on a file pane's own element, so no terminal has the keyboard while it applies (ADR 0108); the
editor's own keys (arrows, Ctrl+Z/Y/A/C/X/V) are not in the keymap.
One exception on Windows and Linux: plain Ctrl+C copies while terminal text is selected (ADR 0022); without a selection it reaches the agent as ^C. Anything sent to the agent clears the selection.
Plain Ctrl+V pastes in a terminal pane; ^V reaches the program only when the clipboard holds no
text, image or files (ADR 0026).
On macOS neither exception applies (ADR 0099): Ctrl+C is always ^C and Ctrl+V is always ^V.
Cmd+C is the Copy command: it copies the selection and does nothing without one (never ^C).
Cmd+V is the Paste command. Other Cmd keys are not sent to the program.
On macOS Option+key types the layout's character by default (Option+2 is €, Option+E then E is é, German Option+L is @).
With `[terminal] option_as_meta = true` Option sends ESC plus the key (Option+B is `ESC b`, Option+Shift+B is `ESC B`), so a shell's word moves work.
Keys that are not text keep their xterm form with Option as Alt (Option+←, Option+Backspace).
The setting has no effect on Windows and Linux (ADR 0099).
The input method (macOS dead keys and Option characters, Windows Telex, CJK) types into a terminal or field as key presses: composing text is typed in place and erased and retyped when the input method revises it (by range on macOS).
With an input mode that is not ASCII-capable (Japanese kana, Chinese, Korean), printable keys reach the input method first; ASCII-capable modes (Telex, VNI, Romaji) get plain letters as typed.
Every text field of the app (task title and prompt, composer, follow-up, commit box, run setup,
comments, broadcast, peek reply, the palette, Ask AI, Settings, the filters and
find bars, the inline rename) edits like a normal text box (ADR 0075): ←/→, Ctrl+←/→ by word,
Home / End, Ctrl+Home / End, ↑/↓ between lines of a multi-line field, Shift to select; Ctrl+A
(select all: the next key replaces it), Backspace / Delete, Ctrl+Backspace / Ctrl+Delete (the
word before / after the caret: blanks, then a run of letters / digits / `_` or of punctuation,
like the Editor), Ctrl+C / Ctrl+X / Ctrl+V, and Ctrl+Z / Ctrl+Y (a typed word is one undo
step). A click puts the caret under the pointer, Shift+click or a drag selects, a double / triple
click selects the word / line. Where a field sits over a list, ↑/↓ move in the list.
Shift+Enter sends `ESC CR` (the Alt+Enter form), which agents read as a new line; Mkrew has no
kitty / win32-input keyboard mode, so this is the one form every agent gets through ConPTY.
On macOS (ADR 0098) fields and the Editor follow the Mac text keys instead: Option+←/→ by word, Cmd+←/→ to the line's start / end, Cmd+↑/↓ to the text's start / end, Option+Backspace / Option+Delete the word before / after the caret, Cmd+Backspace / Cmd+Delete to the line's start / end, Cmd+A / C / X / V / Z, Cmd+Shift+Z (and Cmd+Y) redo, Cmd+Enter or Ctrl+Enter save or send, and the Emacs keys Ctrl+A / E (line start / end), Ctrl+B / F / P / N (← → ↑ ↓), Ctrl+D (Delete), Ctrl+H (Backspace) and Ctrl+K (delete to the line's end); Ctrl+←/→ are left to macOS (Spaces).
An Option + character key types the character macOS composed (Option+B types "∫").
On macOS most defaults use Cmd where the table says Ctrl (Ctrl+Option keys stay), and never need Option with a character key (Option types characters on a Mac), Shift with a digit or symbol (macOS reports the shifted character, so such a binding never matches), an F-key or PgUp / PgDn (Fn on a MacBook); "macOS defaults" below lists every difference (ADR 0098).
Shortcut text on macOS uses ⌃ ⌥ ⇧ ⌘ in Apple's order with no separator (⇧⌘P).

| Group | Action | Default |
| --- | --- | --- |
| Global | Command palette (All scope) | Ctrl+Shift+P |
| Global | Palette on Files: find a file of the active workspace | Ctrl+Shift+O |
| Global | Palette on Search: find text in the files of the active workspace | Ctrl+Alt+Shift+F |
| Global | Open settings | Ctrl+, |
| Global | Toggle theme (System → Light → Dark) | Ctrl+Shift+J |
| Global | Select theme | Ctrl+Alt+J |
| Global | Toggle full screen | F11 |
| Global | Toggle sidebar | Ctrl+Shift+B |
| Global | Toggle compact sidebar | Ctrl+Alt+B |
| Global | Voice input: start / stop recording (§20G) | Ctrl+Shift+D |
| Global | Ask AI: a question outside every project (§20H) | Ctrl+Shift+G |
| Global | Focus the sidebar filter with the active row highlighted (shows the sidebar if hidden, widens the compact rail; again returns to the pane) | Alt+0 |
| Global | Open the context menu of what has the keyboard (the menu a right-click opens: terminal pane, file tree cursor row, editor tab, sidebar row, Board card; else the workspace menu under the switcher); again closes it | Shift+F10 (the Menu key: bind `context_menu` in Settings) |
| Settings | On the Prompts page (page keys of the dialog, not in `keymap.toml`): new prompt / save the open prompt | Ctrl+N / Ctrl+S |
| View | Toggle the right panel (Files / Session / Git tabs; keeps the tab) | Ctrl+Shift+Y |
| Workspace | Jump to workspace 1–9, counted top to bottom as the sidebar shows them (parked empty workspaces after the cards) | Alt+1 … Alt+9 |
| Workspace | Next / previous workspace (same order) | Ctrl+Alt+↓ / Ctrl+Alt+↑ |
| Workspace | Add workspace (folder or clone) | Ctrl+Shift+K |
| View | Terminals / Board / Docs | Ctrl+Shift+1 / 2 / 5 |
| View | Show Git panel (right panel, the Git list gets the keyboard; ADR 0084) | Ctrl+Shift+4 |
| View | Show the diff: opens the Diff tab after the active tab, or focuses the Diff pane where it is open (ADR 0108) | Ctrl+Shift+7 |
| View | Show file tree (right panel, the tree gets the keyboard; ADR 0081) | Ctrl+Shift+3 |
| Inbox | Jump to next / previous item needing attention (sessions asking for input first, then newest first) | Ctrl+Shift+I / Ctrl+Alt+I |
| Task | New task / run selected task | Ctrl+Shift+N / Ctrl+Enter |
| Board | Select a card (↑/↓ in the column, ←/→ the same row of the next column with cards) / move it one column (the same `task.move`s as a drag) | ↑↓←→ / Shift+← Shift+→ |
| Board | Open the selected card (its live session, else its edit dialog) / edit it | Enter / E |
| Board | Approve the selected Review card (also: the same ↑↓←→ and Enter move and open rows of the Jira list and Kanban; ←/→ fold a List row) | A |
| Board | Show Board while the Board is shown and Jira is set up: switch Tasks / Jira | Ctrl+Shift+2 |
| Run | Start all / restart all / stop all (§10 Run) | Ctrl+F5 / Ctrl+Shift+F5 / Ctrl+Alt+F5 |
| Tabs | New shell tab / new agent tab (pick profile) | Ctrl+Shift+T / Ctrl+Shift+A |
| Tabs | Next / previous tab (file and diff tabs are tabs like the others) | Ctrl+Tab / Ctrl+Shift+Tab |
| Tabs | Jump to tab 1–9 (9 = the last) | Ctrl+Alt+1 … Ctrl+Alt+9 |
| Tabs | Rename / close tab (a tab with unsaved files asks Save / Don't save / Cancel) | Ctrl+Shift+R / Ctrl+Shift+W |
| Tabs | Move the active tab one place left / right inside its pinned or unpinned group | Ctrl+Shift+PgUp / Ctrl+Shift+PgDn |
| Tabs | New note: creates a note file in the notes folder (§20.0 `[notes]`) and opens it in a new tab (ADR 0108) | Ctrl+Alt+N |
| Tabs | Move to new window | Shift+F4 |
| Window | Open each workspace in its own window… / Combine into one window (ADR 0115) | command palette only |
| Panel | Move the Files / Session / Git panel to the other side (`[ui] panel_side`) | command palette only |
| Panes | Split right / split down | Alt+Shift+= / Alt+Shift+- |
| Panes | Layout the active tab: 2 columns / 2 rows / 3 columns / 1 + 2 / 2 × 2 (§10) | Alt+Shift+1 / 2 / 3 / 4 / 5 |
| Panes | Equalize pane sizes (§10) | Alt+Shift+0 |
| Panes | New agent pane / new shell pane (split right with the workspace's default agent / shell; the title bar's `+` row and Ctrl+Shift+A / Ctrl+Shift+T create tabs) | Alt+Shift+A / Alt+Shift+S |
| Panes | Move focus (file and diff panes count as panes); past the outer pane (or from Board or Docs) ← enters the sidebar filter and → the right panel's file tree or Git list, only while on screen (the compact rail widens); from those ← / → come back to the center | Alt+←↑→↓ |
| Panes | Resize | Alt+Shift+←↑→↓ |
| Panes | Zoom / unzoom pane | Ctrl+Shift+M |
| Panes | Toggle Terminal / Chat of the focused agent pane (ADR 0104; works while its composer has the keys) | Ctrl+Alt+T |
| Panes | Close pane (asks if agent running) | Ctrl+Shift+X |
| Panes | Rename pane (also: double-click a tab that keeps its default name) | F2 |
| Panes | New browser pane (split right, empty, address field focused; global, also while a page has the keyboard; not offered where the browser is unavailable, §20M) | Alt+Shift+B |
| Browser pane | Focus the address field (selects all; Esc gives the keyboard back to the page) | Ctrl+L |
| Browser pane | Back, Forward, Reload, Open in system browser | command palette only |
| Terminal | Copy / paste | Ctrl+Shift+C (or Ctrl+C while text is selected) / Ctrl+V or Ctrl+Shift+V |
| Terminal / Editor / Docs | Find in scrollback / in the opened file / filter the Docs list | Ctrl+Shift+F |
| Terminal | Page up / down | Shift+PgUp / Shift+PgDn |
| Terminal | Copy mode: select and copy with vi keys (§10) / label the links on screen and open one by typing its label (§10) | Alt+Shift+C / Alt+Shift+O |
| Docs | Comment on selection | Ctrl+Alt+M |
| Docs | Send comments to agent | Ctrl+Shift+S |
| Docs / Git | Pick lines: the first line or block in view (Git: the current hunk's first line), then grow / shrink the pick one line or block (Esc clears; a text field keeps Shift+↑/↓) | Shift+↓ / Shift+↑ |
| Docs | Toggle preview / source | Ctrl+Shift+E |
| Docs / Git | Select the next / previous comment thread (scrolled into view) | F8 / Shift+F8 |
| Docs / Git | Reply to / resolve or reopen the selected thread | Ctrl+Alt+R / Ctrl+Alt+Enter |
| Editor | Toggle preview / source of a Markdown file (no terminal has focus there, so Paste keeps the keys in terminals) | Ctrl+Shift+V |
| Editor | Toggle the Markdown Split view (source and live preview side by side) / back to source | Ctrl+Alt+V |
| Zed | Open workspace or current file in Zed | Ctrl+Shift+Z |
| File tree | Expand/collapse, move | ← → ↑ ↓ |
| File tree | Toggle ignored files (while the tree has the keyboard; the palette entry works anywhere in a workspace) | Ctrl+Shift+H |
| File tree | Open the tree's cursor file in a kept tab (Enter opens a transient one) | Alt+Enter |
| Editor | Save the opened file | Ctrl+S |
| Editor | Find / replace in the opened file (replace all: Ctrl+Alt+Enter in the bar, ⌥⌘↩ on macOS) | Ctrl+F / Ctrl+H |
| Editor | Close the focused file pane (asks about unsaved text; ADR 0080, 0108) | Ctrl+W |
| Git | Next / previous file | ↓ / ↑ (or j / k) |
| Git | Open the selected file's diff | Enter |
| Git | Close an open menu, else unselect the picked lines, else close the diff | Esc |
| Git | Next / previous hunk (past the last / first one: the next / previous file that is not viewed) | n / p |
| Git | Stage / unstage selected file or hunk | s / u |
| Git | Toggle Working changes / Against base | b |
| Git | Toggle History | h |
| Git | Toggle Unified / Split diff | d |
| Git | Wrap long diff lines / cut them at the edge | W |
| Git | Mark the selected file viewed / not viewed (marking opens the next file that is not viewed) | v |
| Git | Focus commit message / commit | c / Ctrl+Enter |
| Git | Stage all of the selected file's group (Unstaged + Untracked when a Staged file or nothing is selected) / unstage all | Shift+S / Shift+U |
| Git | Discard the current hunk, else the selected file (asks first) / fetch / push | x / f / Shift+P |
| Git | Comment on selected lines | Ctrl+Alt+M |
| Git | Send comments to agent | Ctrl+Shift+S |
| Agent (v2) | Peek the first session that needs you (↑/↓ cycle, Alt+Y / Alt+N answer, Ctrl+O (⌘O on macOS) open the pane, Esc close); hovering a row also peeks | Ctrl+Shift+Space |
| Agent (v2) | Open the prompt composer of the focused agent pane | Ctrl+Shift+Enter |
| Agent (v2) | In a prompt box (the composer, the task dialog's Prompt), at the start of a line or after a space: pick the agent's command (§20B.7) / pick a file (§12A); Enter or Tab types the pick, Esc types the character | / / @ |
| Agent (v2) | Send selection to an agent (Editor, Markdown preview, Docs, Git diff, terminal) | Ctrl+Shift+L |
| Agent (v2) | Broadcast, prompts, follow in Zed, turns, fan-out, new pane of a chosen agent profile ("Agent: New <Profile> pane") | command palette only |
| Task / Workspace | Merge back / remove worktree of the selected task; pin, open in File Explorer (Reveal in Finder on macOS), set default agent, remove, workspace menu of the active workspace; tab pin / close others / close to the right, pane pin and reading width, workspace set default shell and move up / down, agent stop / open transcript / link plan / unlink plan (only with a linked plan) / improve prompt, run start / restart / stop of one command, Git change base (only in Against base) and switch repository, Jira all / mine, list / kanban and refresh, copy last error, dismiss notifications, History (needs a workspace), install update, sidebar "Remove idle agents and terminals (N)" (only while N is above 0) and "Collapse all workspaces" / "Expand all workspaces" (named for what the toggle does now; ADR 0106) | command palette only (the menu items also by Shift+F10, ⌃⌘M on macOS) |

**macOS defaults** (ADR 0098); a command not listed keeps the key of the table above:

| Action | Default on macOS |
| --- | --- |
| Add workspace / switch to workspace 1-9 / focus the sidebar | ⇧⌘K / ⌘1-9 / ⌘0 |
| New shell tab / new agent tab / rename tab / close tab | ⌘T / ⇧⌘A / ⇧⌘R / ⇧⌘W |
| Split right / split down / split with agent / split with shell | ⌘D / ⇧⌘D / ⌥⌘A / ⌥⌘S |
| New browser pane / focus address (browser pane) | ⌥⌘B / ⌘L |
| Layouts 1-5 / equalize panes | ⌥⌘1-5 / ⌥⌘0 |
| Focus pane ← → ↑ ↓ / resize pane | ⌥⌘ arrows / ⌥⇧⌘ arrows |
| Zoom pane / close pane / rename pane | ⇧⌘M / ⇧⌘X / ⌥⌘R |
| Terminal copy / paste / page up / page down / copy mode / link hints | ⌘C / ⌘V / ⌘↑ / ⌘↓ / ⌥⌘C / ⌥⌘O |
| Find / Open in Zed / next Inbox item / peek | ⌘F / ⌥⌘E / ⇧⌘I / ⇧⌘Space |
| Composer / chat view / send selection / Ask AI / voice input | ⇧⌘↩ / ⇧⌘T / ⇧⌘L / ⇧⌘G / ⌃⌘V |
| Terminals, Board, File tree, Git, Docs / Diff | ⌃⌘1 … ⌃⌘5 / ⌃⌘7 |
| Docs source / send comments / next / previous comment | ⇧⌘E / ⇧⌘S / ⌘] / ⌘[ |
| File tree: show ignored | ⇧⌘H |
| Editor: save / find / replace / close pane / preview | ⌘S / ⌥⇧⌘F / ⌥⌘F / ⌘W / ⇧⌘V |
| Editor find bar: replace all | ⌥⌘↩ |
| Search in files | ⇧⌘F |
| Quick open / new task / run task (Board) | ⌘P / ⌘N / ⌘↩ |
| Run start / restart / stop | ⌘R / ⌃⌘R / ⌘. |
| Command palette / settings / toggle theme / full screen | ⇧⌘P / ⌘, / ⇧⌘J / ⌃⌘F |
| Sidebar / right panel / context menu | ⌘B / ⇧⌘Y / ⌃⌘M |
| Move tab left / right / to a new window | ⌃⌘← / ⌃⌘→ / ⇧⌘N |
| New note | ⌥⌘N |

In a file pane, and while the Git list (or the diff, with the list hidden) has the keyboard, the terminal never has focus, so single-key shortcuts are safe there.
Alt+Shift+A / Alt+Shift+S and F2 no longer reach a focused terminal (Alt+Shift+A / S used to be
ESC A / ESC S, a readline binding); rebind `split_agent` / `split_shell` / `rename_pane` in
Settings → Keyboard if needed.
Alt+0 and Ctrl+Shift+PgUp / PgDn no longer reach a focused terminal.
In the Board, the Git tab and the file tree the single keys `a`, `w` and Alt+Enter act only while the card board, the changed-file list (or the diff, with the list hidden) or the file tree has the keyboard; a text field, a comment draft or the Editor keeps them.
Shift+F10 does not reach a focused terminal either; rebind `context_menu` if a program needs it.
Alt+Shift+C and Alt+Shift+O no longer reach a focused terminal either (`ESC C`, `ESC O`); rebind `copy_mode` / `link_hints` in Settings → Keyboard if a program needs them.
The keys of copy mode (`h j k l w b e 0 $ g G v V y`, arrows, PgUp / PgDn, Enter, Esc) and the letters of link hints are not in `keymap.toml`.
While a popover menu is open it has the keyboard (key context `Menu`, not in `keymap.toml`):
↑ / ↓ select the next / previous enabled row, wrapping.
Home / End select the first / last.
← / → move between the swatches of a color row.
Enter or Space run the row and close the menu (a multi-select row of the fan-out picker ticks and stays; Enter on its Run row runs).
A letter selects the next row starting with it.
Esc closes the menu and gives the keyboard back.
A held (auto-repeat) key does nothing.
The Board's single keys stay with the menu while it is open.

Modal dialogs (ADR 0085): Tab and Shift+Tab move through the dialog's controls and wrap inside it.
The order is the dialog's text fields first, then its options (chips, checkboxes), then its buttons left to right.
Enter or Space on a focused control clicks it; ← / → move between the chips of a row; Esc closes the dialog from anywhere in it.
Tab is not in `keymap.toml`: like the keys of the confirm prompt it only means something inside a dialog.
A destructive button (Discard changes, Remove worktree, Discard files, Git Discard) is never the first focus and does not run on the Enter that opened the dialog.
It runs when Tab moved onto it and Enter or Space is pressed; an auto-repeating key does nothing.
Turns: D shows the changes since the selected turn.
Broadcast: ↑ / ↓ move the keyboard into the session list and pick a session; Space checks it only while the list has the keyboard.
Typing leaves the list, so a space can still be typed in the message.
Run setup: Shift+Tab walks back; in its folder picker ↑ / ↓ move, → opens a folder, ← goes up, Enter takes the folder or env file under the cursor.

Invalid or duplicate bindings: report the error and fall back to defaults for that entry. The
command palette shows each command's shortcut; tooltips show shortcuts too.

The palette has scope tabs: All, Commands, Sessions, Files, Search, Branches, Commits.
Tab and Shift+Tab switch scope, Up and Down move, Enter runs, Esc closes, a click on a tab switches to it.
All lists, for a query, a section per scope except Search (five rows each, then "+N more ›" which switches to that scope); with an empty query it lists the sessions (waiting ones first) and the commands run lately.
Files asks `fs.find` (debounced 80 ms), Search asks `fs.search` from two characters, spaces at the ends not counted (150 ms, plain text, smart case), Branches asks `git.branches` and Commits asks `git.log` (500 commits) once per opening and both are filtered in the UI.
A late answer is dropped.
Branches switches with `git.checkout`; a question comes first when the repository has changed files or a live agent session runs in that folder; git's own error is shown as a toast.
Commits opens the Git view's History on the commit.
A search hit opens the file with the caret on its line.
`@` in a prompt opens the palette on Files in insert mode: Enter types the path as `composer.file_ref`, Esc types the `@`.
The theme picker (`Ctrl+Alt+J`) is the same palette without tabs.

---

## §14 UI: visual design system

Goal: simple, beautiful, easy — soft, never rigid. Feel references: Linear, Zed, Raycast.
The terminal is the hero; everything else steps back.

**Principles**

1. **Little chrome, lots of air.** Separate regions with spacing and subtle background shifts, not
   heavy lines. Borders, if any, are 1 px and faint.
2. **Soft.** Moderate corner radius; very light shadows only on floating layers (popover, dialog,
   palette). No saturated colors besides the accent and status colors. Workspace colors
   (ADR 0020) and pane colors (ADR 0037) are the exception: a small tinted letter avatar per
   workspace and a 3 px stripe on its sidebar card, and a 2 px line of its color at the top
   of a colored pane.
3. **One accent color** for selection, primary buttons, focus. Status colors only for agent states.
4. **Progressive disclosure.** Secondary actions on cards and rows appear on hover/focus.
5. **Gentle motion.** Panels, view switches, drag & drop: 120–180 ms ease-out. Nothing loops while an agent works: its mark is a still dot and its time text moves (ADR 0086).
   Respect the OS reduce-motion setting.
6. **Helpful empty states.** One short sentence plus one primary action with its shortcut.
7. **Keyboard = mouse.** Everything clickable has a visible focus ring; tooltips show shortcuts.

**Design tokens** — all in `crates/mkrew-ui/src/theme.rs`. No hard-coded values in components.

Restyle of 2026-10-24 (ADR 0074). The sidebar session list and compact rail keep the palette from
before it (`Theme::classic`).

| Token | Dark | Light |
| --- | --- | --- |
| `bg` (content) | #15171B | #FFFFFF |
| `surface` (panel) | #121418 | #FAFAFB |
| `surface-raised` | #2A2E35 | #E6E8EC |
| `chrome` / `sidebar` | #0F1013 | #F7F7F8 |
| `card` (raised) | #1C1F24 | #F7F8FA |
| `hover` | #1F2227 | #EEF0F3 |
| `row-hover` | #1A1D21 | #F4F5F7 |
| `well` | #17191D | #F1F2F4 |
| `selected` | #2A2E35 | #E6E8EC |
| `border` | #23262C | #DFE1E6 |
| `control-border` | #262A31 | #D9DCE1 |
| `divider` | #1E2126 | #E9EBEE |
| `terminal-bg` | #0B0C0E | #FCFCFD |
| `pane-gap` (the frame behind and between panes, and the title bar) | #000000 | #F3F4F6 |
| `text` | #ECEDEF | #1B1D22 |
| `text-dim` | #D5D8DD | #3A3E46 |
| `text-muted` | #A3A8B0 | #5A606B |
| `text-subtle` (dimmest text) | #7D838C | #6B717C |
| `text-disabled` | #8A9099 | #6B717C |
| `subtle` (graphics only) | #5D616C | #A0A4AF |
| `accent` | #8E8CF5 | #5452D0 |
| `accent-text` | #C2C1FA | #4A48C4 |
| `accent-bg` | #26273A | #EBEBFA |
| `on-accent` | #0F1013 | #FFFFFF |
| `info` / `info-text` / `info-soft` | #6AA6F2 / #8DB9F5 / #93BCF6 | #2F6FC4 / #2A62AD / #2F6FC4 |
| `teal` | #74D6C4 | #1A7F70 |
| `folder` | #8FA3C0 | #5F7590 |
| `warning-bg` / `success-bg` | #322B23 / #1E302B | #F4EDE2 / #E5F0EA |
| `danger-bg` | #342526 | #F8E8E6 |
| `danger-text` / `success-text` | #EF8F82 / #6FD39B | #B4382A / #1E6B3A |
| `status-working` | #8E8CF5 | #5452D0 |
| `status-needs-input` (attention) | #E4A452 | #8B570D |
| `status-done` (success) | #57C78A | #23854F |
| `status-exited` (danger) | #EF7B6C | #C2412F |
| `status-idle` | #7D838C | #8A8F99 |
| `diff-add-bg` / `diff-del-bg` | #182422 / #261D20 | #E7EFEC / #F5E9E9 |
| `diff-add-word-bg` / `diff-del-word-bg` | #254638 / #543331 | #C4DDD0 / #ECCCC8 |
| `diff-add-text` / `diff-del-text` | #6FD39B / #EF8F82 | #1E6B3A / #A61B1B |
| `diff-hunk-header` | #17191D | #F1F2F4 |
| `qr-dark` | #101114 | #101114 |
| `qr-light` | #FFFFFF | #FFFFFF |

`pane-gap` is darker than `terminal-bg` in dark themes when `terminal-bg` leaves room, a step darker in light themes, and always differs from `terminal-bg` by at least 1.06:1 (`theme::problems()` rule "pane gap"); a generated theme derives it with a mix from `terminal-bg` toward black or `text`.

Tints (chip and pill fills) are a color at ~14 % (dark) / ~12 % (light) alpha.

- Spacing scale 4 · 8 · 12 · 16 · 24 · 32 px. Card padding 12, gap between cards 8, main padding 16–24.
- Radius 5 (area chips, key hints), 6 (chips, segments, tree rows), 8 (buttons, inputs, rows),
  9 (segmented container), 10 (cards), 12 (dialogs, palette), 999 (pills); 4 for old badges.
- Heights: controls 30, inputs 32, title bar 42 (it holds the tabs, ADR 0106), the right panel tab row 44 with 28 px icon tabs, Git repo strip 42, Git section tabs 36, Files tree rows 24, Git file rows 28 (px at 13 px body; ADR 0110).
- Tab chip (Tern look, ADR 0106, ADR 0116): height `TAB_CHIP_HEIGHT` (26), radius 8 (`radius::MEDIUM`), no border, META text at regular weight; the active chip has a solid `tab_active` fill and `text` (`tab_active` always differs from `chrome`, which `theme::problems()` checks); the others are transparent with `text_muted` and a hover fill through `.pressable(Look::Chip, ..)`; the × shows on the active chip and on hover for the others.
  A chip shows one icon, its focused pane's, in `text_subtle` (`text_muted` when active), never a brand color; a working or needs-input tab colors that icon `status_working` / `status_needs_input` instead of adding a glyph, and its tooltip names the state.
  No rule separates pinned tabs from the others.
  Chips shrink to `TAB_MIN_WIDTH` (120, room for the pane icon, about five title letters and the ×) before the tab area scrolls; `TAB_TITLE_MAX_WIDTH` is the maximum.
- Pane (ADR 0106, ADR 0108, ADR 0109): no header and no border.
  Panes fill the tab area edge to edge with square corners: no gap between them and none at the area's edge, so they touch the sidebar, the right panel and the title bar.
  Two panes meet at a 1 px `divider` line, the same line that sets the sidebar and the right panel apart.
  One `PANE_ACCENT_WIDTH` (2 px) stripe runs along the pane's full top edge.
  Its color, first match wins: `status-needs-input` while a session pane needs input, the pane's own color, `pane-focus-stripe` while the pane is focused and its tab has two or more panes, else no stripe.
  A lone pane has no stripe unless it needs input or has a color.
  A pane that is not focused, in a tab with two or more panes, is dimmed by 20 % (`unfocused_pane_dim`).
  File and diff panes use the same layout, stripe and dim as terminal panes.
  The divider between two panes drags to resize them and shows a grip only on hover.
  A pane in a split tab has a drag handle at the centre of its top edge (ADR 0110): while the pointer is over the pane a `PANE_GRIP_BAR_WIDTH` × `PANE_GRIP_BAR_HEIGHT` (32 × 4) muted bar shows; over the handle it becomes a `PANE_GRIP_WIDTH` × `PANE_GRIP_HEIGHT` (40 × 20) `surface_raised` chip with a `grip-horizontal` icon (two rows of three dots), and dragging it moves the pane. Only the handle takes the mouse.
  Floating badges (`pane_badges`: `surface_raised` fill, 1 px `border`, radius 8, META text) sit in its top-right corner `space::S` from the edges, at most `PANE_BADGE_MAX_WIDTH` wide.
  The title bar uses `pane-gap` instead of `chrome`; the sidebar and the right panel keep their colors.
  The `+` row (§10) is a row of 30 px `Look::Icon` buttons: the `+` button, then the default agent's logo, `square-terminal`, `notebook-pen` and `chevron-down`, `space::XS` apart, growing in 150 ms ease-out (shown at once with reduce motion).
  The title bar's pane toggles are 30 px icon buttons, accent while on.
- Shared controls (`shell/widgets.rs`): segmented control, primary / secondary / ghost / destructive
  buttons, 30 px icon button, panel icon tab, underlined tab, search box, status pill, tinted chip, overline, key hint.
- UI font = Geist (OFL, bundled in `font/geist/` and registered at startup, ADR 0116) unless
  `[ui] font_family` is set. Sizes 11 overline, 12 meta, 13 body, 15 card
  title, 20 view title at the default `[ui] font_size = 13`; other sizes keep their ratio to body
  (ADR 0024). Weights 400/500/600. All-caps only for 11/600 overlines. UI line height 1.5 (`UI_LINE_HEIGHT`, set on every window root).
- Docs preview (ADR 0021): body 14 px, line height 1.5, headings 24/20/17/15/14/14 px, code 13 px,
  a centered column of at most 960 px (ADR 0083). A drawn Mermaid diagram shows "Expand" at
  its bottom-right on hover: a dialog of 90% of the window shows it as wide as fits, up to 2x
  its natural size (a tall one scrolls); Esc or Close closes it.
- Terminal font defaults: JetBrains Mono → Cascadia Code → Cascadia Mono → Consolas (Windows),
  JetBrains Mono → SF Mono → Menlo
  (macOS), JetBrains Mono → DejaVu Sans Mono (Linux): the first installed one, 13 px;
  `[terminal] font_family` / `font_size` override them (a family that is not installed is
  ignored). Icons of
  Nerd Font prompts/TUIs (private-use code points) come from an installed Nerd Font "Mono"
  family (`theme::TERMINAL_GLYPH_FALLBACKS`), then the system fallback. ANSI palette
  harmonized with the theme.
- Terminal: `terminal-bg` fill, 16 px padding on every side, line height 1.75;
  bar cursor in the text color (hollow block when the pane is unfocused; a program's DECSCUSR
  shape wins); a pane has no border; the top stripe marks needs-input, a color and the focused split pane.
  Editor code rows keep line height 1.35 (`code_line_height`).
- Icons (ADR 0051): icon-only buttons, toggles, window controls, row actions and row chevrons
  draw Lucide SVG icons (`icon/lucide/`, `crates/mkrew-ui/src/icons.rs`) in a theme token color with stroke 1.5 (`ICON_STROKE`, ADR 0116),
  16 px in icon buttons, 14 px among text and 12 px for chevrons, tab and pin icons (`ICON_SIZE`, `ICON_SIZE_SMALL`, `TAB_ICON_SIZE`). The glyphs this
  spec names for those buttons map to Lucide: `+` plus, `▾ ▸ ‹ ›` chevrons, `⋯` ellipsis, `◷`
  history, `⌫` delete, `◐ ☀ ☾` sun-moon / sun / moon, `─ □ ❐ ✕` minus / square / copy / x, `⌕`
  search, `◫` columns-2, `◧ ◨` panel-left / panel-right, `⇤ ⇥` panel-left-close / -open,
  `↻ ■ ▶` rotate-cw / circle-stop / play, `✦` sparkles, Fetch `↓` arrow-down-to-line. A pane has no
  close button: Close pane is in the tab menu and the palette. Frequent toolbar, card and
  empty-state actions carry a 14 px icon before their text (Share, idle Run, Save, Find,
  Comment, Send to agent, Open in Zed, Push, …; the list is in ADR 0051); a `▾` after a label
  is a chevron-down icon. Dialog footers, Retry, thread actions, row actions and danger buttons
  stay text. Status icons (§11), marks in messages and the live Run label stay text.
- Phone access (ADR 0053): the palette command "Global: Phone access…" (ADR 0106 removed the title bar's `smartphone` icon button) opens the "Phone
  access" dialog (`DIALOG_WIDTH`, same frame as the other dialogs, Esc / Close). Rows, each
  with a glyph and text (§11): **Status** `○ Off` + Start, `● On` + Stop, `✕ Not running:
  <error>. Retrying…` + Stop; **Tailscale** `✓ Tailscale serve is set up`, `Tailscale doesn't
  publish Mkrew yet` + Set up (its error below), `Install and sign in to Tailscale first`; **Link**
  (only while On and the MagicDNS name is known): the QR code, the link as one line and Copy
  link, "Scan with the iPhone camera, then Share → Add to Home Screen." (`Starting…` until
  `mk web` has written `web_token`). The QR is always dark modules (`qr-dark`) on a light
  panel (`qr-light`) in both themes, `PHONE_QR_SIZE` px. The dialog re-reads `daemon.status`
  and Tailscale about once a second while open (off the UI thread). A failed Start / Stop shows in the dialog. The link carries the
  access token: it is never logged and never in a tooltip.
- Settings (Ctrl+,, palette "Global: Open settings", ADR 0073, ADR 0097): a floating glass panel (`SETTINGS_DIALOG_WIDTH` ×
  `SETTINGS_DIALOG_HEIGHT`, never larger than the window minus a margin). The left navigation (`SETTINGS_NAV_WIDTH`) starts with a
  search box (Ctrl+F focuses it) and lists the pages under three overlines: App (General, Appearance, Notifications, Voice, Features),
  Integrations (Jira, Phone, Machines) and Input (Keyboard, Prompts). The right side has the page title and a close button.
  With a query, the right side lists the matching settings of every page (a muted `Page › Group` breadcrumb, the label with the
  matched characters in accent and semibold, the help line and the real control); a setting matches when `palette::fuzzy` scores its label,
  help, TOML key or page name above zero; a label match always ranks above a help-only match, then by score; pages with no match are dimmed; shortcuts are not searched.
  Esc clears a non-empty query first and closes Settings on an empty one; no match shows "No setting matches “<query>”." with
  "Clear search (Esc)". Each page shows its settings in cards, one per group (`Setting::group`: an overline title and a card
  of rows with faint dividers; label and muted help on the left, the control on the right); the Keyboard page is one card of all its shortcut rows. Controls: switch (`toggle`), segmented
  choice, `dropdown_trigger` (fonts, themes) and text fields for Text and Number; Number and Text rows are
  written on Enter or when the row loses the keyboard, Esc drops the draft. Keyboard rows show a
  chip, an accent dot when changed and Reset; a click or Enter captures the next keystroke, Esc
  cancels. A refused value or duplicate keystroke shows under its row, never as a toast. A file that
  does not parse shows a banner with "Fix in Zed" and the settings controls do nothing (switches, segments and dropdowns are faded, with no hover and no pointer). The footer
  shows `v<version>`, the links `settings.toml` and `keymap.toml` (they open the files in Zed) and "Changes apply at once."; on the Prompts page the hint stays "Ctrl+N new · Ctrl+S saves · Esc closes".
- Palette (Ctrl+Shift+P, ADR 0097): a floating glass panel (`floating_panel`, `PALETTE_WIDTH` wide).
  The panel is anchored at `PALETTE_TOP_OFFSET` from the top, so it does not move when the list height changes.
  Top: the query field with a search icon, the scope's placeholder and an `esc` key cap (`search_box`).
  Below it: the scope tabs as pills (`Look::Chip`; the current one has a 1 px accent border and no fill, the others are muted and get the hover fill).
  The list is at most `PALETTE_LIST_HEIGHT` tall and scrolls.
  It has section overlines, `list_row` rows (the selected one has the `selected` fill and a `PALETTE_SELECTED_BAR` accent bar) and a muted "+N more ›" row; matched characters are accent and semibold, `detail` is muted after the title, `age` is right-aligned, shortcuts are `kbd` key caps.
  A scope that waits shows a muted "Loading…" and one that failed a muted error line with an `x` icon, in its own tab only.
  Footer: "↑↓ navigate · ↵ run · Tab next scope" on the left and "esc close" on the right.
  The theme picker and the `@` file picker have no tabs; the theme picker keeps its muted "Dark" / "Light" text instead of key caps.
  The theme picker footer reads "↑↓ preview · ↵ save" and "esc restore".
  The `@` footer reads "↑↓ navigate · ↵ type its path" and "esc type @", under the title "Insert file · <workspace>".
- Settings → Machines (ADR 0089, this computer's window; a machine window shows only "Machines are managed in This computer's window."):
  one row per machine of `<config>/mkrew/machines.toml`: name, host (and the `mk` path), a status dot with its state in words (Connected, Connecting…, Reconnecting…, Not reachable: <reason>, Version differs: <message>, Window closed), **Open** (raises the window, or starts it) and **Remove** (confirms, closes that window, deletes the line).
  **Add machine…** (a bordered button; the filled primary one while the list is empty) opens a card with Name, Host and "mk path (optional)"; a refused value shows under its field, Save writes the file and opens the window.
  A `machines.toml` that does not parse shows its error with file and line instead of the rows (nothing is written), and is read again while the tab is open, so a fix by hand clears it.
  A line under the list: "The other computer needs Mkrew, an SSH server and this computer's SSH key."
  Palette: "Machine: Add…" and "Machine: Open <name>".
  The tray (Windows) has one item per machine, `<name> - <state>` (Connected, Connecting…, Reconnecting…, Not reachable, Version differs, Window closed), that opens or raises its window.
  In a machine window every Settings page starts with "Retention, features and other daemon settings are <name>'s own; change them on <name>. Notifications from this window follow this computer's settings."
- Settings → Prompts (spec 2026-11-04, SPEC §20B.3): the prompt library `<config>/mkrew/prompts` without a file manager.
  The folder is read when the page opens and after every write; a prompt file that cannot be read shows its error in its own row.
  A machine window edits this computer's folder and says so in a line at the top.
  Left, `SETTINGS_PROMPTS_LIST_WIDTH` wide: a **New prompt** button (Ctrl+N) and one row per prompt, its `# Title` (else its name) and, when different, its name in `text-muted`; the open prompt has the raised fill and the one the keyboard is on the selected ring; ↑ / ↓ open the neighbour.
  An empty library shows "No prompts yet." and a filled **New prompt** button.
  Right: a Name field, the prompt text in a multi-line box (`SETTINGS_PROMPT_BODY_HEIGHT`, ADR 0075; the `# Title` line is part of the text), a hint naming `{file}`, `{selection}`, `{task}` and `{project}`, **Save** (Ctrl+S, or Ctrl+Enter in a field; disabled while nothing changed), **Delete** (asks first; disabled for a prompt not saved yet) and **Open folder** (in Zed).
  Tab walks the page list, the prompt list, Name, the text and the buttons; Enter in Name moves to the text.
  A refused name (empty, over 64 characters, a leading dot, one of `/ \ : * ? " < > |`, a control character, a trailing space or dot, a Windows reserved name such as `CON` or `aux.v2`, or another prompt's name in any case) shows its reason under the field; a failed write shows at the top of the page; neither closes the page.
  Renaming to another case of the same name is allowed.
  Switching prompt, New prompt and Esc ask "Save changes to …?" (Save, Don't save, Cancel) while the name or the text has unsaved changes; switching to another Settings page keeps the draft.
  Palette: "Global: Manage prompts…" (also in a machine window); the prompt picker's last row "Manage prompts…" opens the page too.
- Machine chip and reconnect banner (ADR 0089) add no tokens; they reuse these.
  Chip: `well` fill, `text-muted` icon and name, `radius::PILL`, `PILL_HEIGHT` (22), `ICON_SIZE_SMALL` (14), `META` text, `space::XS` between parts, `TITLE_CHIP_MAX_WIDTH` (160).
  Dot (`status_dot`, also on the Machines rows): `status-done` #57C78A / #23854F, `status-needs-input` #E4A452 / #8B570D, `status-exited` #EF7B6C / #C2412F, `text-muted` for a closed window.
  Banner: `warning-bg` #322B23 / #F4EDE2 fill, a `BORDER_WIDTH` bottom edge in `divider`, `space::M` x `space::S` padding, `META` text in `text` (the detail line in `text-muted`), **Try now** a primary button.
  The Machines rows and Add card use `space::S` / `space::M`, `radius::MEDIUM` and `surface-raised`; a broken-file error shows `status-exited` ✕ and `text`.
- Density: one "comfortable" density in v1 (a compact option is backlog).
- Focused pane: a 2 px stripe of `pane-focus-stripe` at its top edge when its tab has two or more panes (needs-input and a pane color win); unfocused panes dimmed ~5%.
- Focus ring (ADR 0085): a dialog control that has the keyboard shows a 1 px border in `focus-ring` (the text color) over its own border, only while it was reached with the keyboard (`focus_visible`), in both themes.
  A click never shows it.
- Derived tokens (accent mixed into `bg`): `pane-focus-stripe` #49518D / #A4AAE7 (also the focus border of cards, comments and inputs);
  `terminal-selection-bg` #3A406B / #CFD2F0 (selected terminal text is drawn in `text`).
  Find matches (`status-needs-input` amber mixed into `bg`, text drawn in `text`):
  `terminal-match-bg` #3F3524 / #F6EBCF; `terminal-match-current-bg` #7A5A1F / #E9C46A.
- Workspace colors (red, orange, yellow, lime, green, teal, blue, purple, pink): dark #F87171
  #FB923C #FACC15 #A3E635 #4ADE80 #2DD4BF #60A5FA #C084FC #F472B6 / light #DC2626 #D9560B
  #A16207 #4D7C0F #15803D #0F766E #2563EB #9333EA #DB2777 (≥ 3:1 on every surface; neighbouring
  hues ≥ 12° apart).
- Update button (§20K): Lucide `circle-arrow-up` in `accent-text` in the title bar right before
  the Board icon while a newer release is known; dimmed (`opacity::DISABLED`) and not clickable
  while it downloads, with "Updating…" shown beside it.
- In-app toasts (`crates/mkrew-ui/src/toast.rs` queue, `shell/toasts.rs` card): bottom-right over
  the content, at most 3 (a fourth drops the oldest; the same message again restarts its toast),
  each hides after about 4 s (`TOAST_DURATION`, expired on the 500 ms status tick, so 4.0–4.5 s) or
  on its ✕; hovering a toast pauses its expiry and leaving restarts the full time. Clicking an error
  toast's text copies the full text and shows an info toast "Copied". Never blocking: no focus, no animation, only the card itself takes clicks. Two kinds,
  always icon + text: info (Lucide `info`, `accent-text`) and error (Lucide `circle-alert`,
  `status-exited`). Used for results of an action that has no place of its own (a failure after its
  dialog closed or its view moved, a failed settings write, a copy, and one-off workspace actions:
  open in Zed / File Explorer, add / remove / update a workspace, run / stop); a failure that has one (a
  dialog's error row, the commit box, a card notice) stays there; the tab strip's errors and notices are toasts (ADR 0106).
- Confirmations (`window.prompt`) are drawn in the window, never as the OS message box
  (`App::set_prompt_builder`, `shell/confirm_prompt.rs`): the modal shade, a `surface` card
  (`CONFIRM_DIALOG_WIDTH`, radius 12, faint border, light shadow), the question as a card
  title, the detail in `text-muted`, buttons on the right with Cancel leftmost and the first
  answer rightmost (accent; the danger button for `PromptLevel::Critical`). Esc = Cancel,
  Enter/Space = the ringed button (default: the first answer), Tab/←/→ move the ring. A click
  on the shade does nothing. OS file/folder pickers stay native.
- Theme: System (follows the OS live), Light or Dark - `[ui] theme`, cycled by Ctrl+Shift+J and the palette
  (ADR 0027).
  Each slot shows a theme (ADR 0078, ADR 0101): `[ui] light_theme` (default Mkrew Light) and `[ui] dark_theme` (default Mkrew Dark).
  There are 22 built-in themes.
  Dark: Mkrew Dark, One Dark, Dracula, Catppuccin Mocha, Tokyo Night, Gruvbox Dark, Nord, Solarized Dark, GitHub Dark, Rosé Pine, Kanagawa, Everforest Dark.
  Light: Mkrew Light, One Light, GitHub Light, Catppuccin Latte, Solarized Light, Gruvbox Light, Rosé Pine Dawn, Everforest Light, Ayu Light, Tokyo Night Day.
  A file `<config>/mkrew/themes/*.toml` adds one (`name`, `dark` and the fields of the built-in palette, all required; the folder is polled with `settings.toml`, so a saved file shows up within a second).
  A file whose name is a built-in's name, or an earlier file's name, is an error; the comparison ignores ASCII case.
  An invalid file shows its error on a card and never breaks the app; a file with poor contrast loads with a "Low contrast" badge.
  A name that is unknown, or whose file is gone, shows the slot's Mkrew theme.
  Settings → Appearance opens with the theme gallery above its cards: "Dark" and "Light" groups of cards (a small window in the theme's own colors, its name, badges), "New theme from current" and "Open themes folder" under them.
  Hovering a card for 120 ms, or focusing it with the keyboard (Tab reaches the gallery, arrows move between cards), shows the whole app in that theme; leaving the gallery, Esc or closing Settings restores; a click or Enter keeps the theme for its own slot.
  "Theme: Select theme…" (Ctrl+Alt+J) lists the same themes, previews while moving, Enter or a click on a row saves to the slot shown, Esc restores (an outside click does not close it).
  "Theme: New theme from current" and "Theme: Open themes folder" in the palette do what the two buttons under the cards do; the buttons are Tab stops with a focus ring and a tooltip.
  Settings → Appearance also has "Light mode theme" / "Dark mode theme" rows.
  In terminals the default fg/bg and ANSI 0–15 follow the theme;
  256-color 16–255 and 24-bit colors are drawn as the program
  sends them.

**Design check (every milestone with UI):** capture screenshots of new views in light and dark
into `docs/screenshots/` (if automatic capture is not possible, list them in `docs/HUMAN_CHECKS.md`)
and self-review: no hard-coded colors/spacing; hover/focus/disabled states exist; empty states
exist; body text contrast ≥ 4.5:1; status never conveyed by color alone; run action glyphs ≥ 3:1.

**Floating panels (glass), ADR 0097.**
The palette, Settings, every modal dialog and the confirm prompt share one frame, `widgets::floating_panel`.
It is a translucent fill (`glass`: the surface at 0.96 alpha in dark themes and 0.97 in light ones, raised until body text keeps 4.5:1 over a light or a dark page behind it), a 1 px `glass_border`, an accent `glass_tint` gradient over the top `GLASS_TINT_HEIGHT` px, `shadow_float` and `radius::LARGE`.
There is no real backdrop blur (gpui-pre 0.3.7 cannot blur behind one element).
The scrim behind a modal is black at 24 % (dark) or 16 % (light).
A panel fades in and rises 4 px over `motion::FAST` and disappears at once; with the OS "Animation effects" setting off (Windows) it appears at once.
No dialog builds its own panel frame.

**Pressable states, ADR 0097.**
Every element with a click or mouse-down handler gets its hover, press and disabled look from `shell/widgets.rs` and `mkrew_ui::pressable` (`Look`, `press_style`, `Pressable`).
It is either a widget (`button`, `secondary_button`, `quiet_button`, `danger_button`, `icon_button`, `segment`, `menu_item`, `row_action`, `list_row`, `toggle`, `dropdown_trigger`) or `.pressable(look, enabled, theme)` with `.on_press(enabled, handler)`.
Only `widgets.rs` and `pressable.rs` call `.hover(` and `.active(` or set `.cursor_pointer()` by hand.
`group_hover(..)`, which reveals hidden row actions, is allowed anywhere.
The looks are `Row` (list and menu rows), `Chip` (pills, tabs, small labelled controls), `Icon` (icon-only controls), `Text` (links) and `Raised` (a control on a raised surface or inside a row that is itself hovered: menu, toast, card, find bar, code block, a row action).
Board cards use `card_press`: the border lights on hover and the fill stays, so the chips inside a card keep a visible hover.
A hover fill is never the selected fill.
A selected or active element that is still clickable keeps the pointer only (`pressable_unless(picked, look, theme)`).
A disabled element is faded (`opacity::DISABLED`), has the default cursor and no hover, no press and no click.
An element that is not clickable right now, but is not disabled, has no look at all.
Not buttons, so no hover: resize grips (they tint through `grip_hover`), terminal surfaces, drop zones and drag sources, the modal scrim, text fields (they keep their I-beam), editor and text-selection surfaces, window-drag areas, the pane title (it renames on double-click only and shows no pointer), links drawn by `InteractiveText`, and the context-menu mouse down of an element that is already pressable.
Focus rings stay with `DialogControl`.

**TSP style mapping, ADR 0107.**
Every color, space and font in an omp surface (§11) comes from `theme.rs`; omp's `t` palette message and its dark flag are ignored, and the UI tells omp its own theme.
The mapping is one table in `crates/mkrew-ui/src/tsp.rs` (`color`, `tone_color`, `span_style`, `icon`), unit tested.
The first style token of a span that has a color decides the color; an unknown token changes nothing.

| omp token (span `s`) | Drawn as |
| --- | --- |
| `accent`, `info`, `link`, `user` | `accent` |
| `muted`, `dim` | `text_muted` |
| `error` | `danger_text` |
| `warning`, `pending` | `status_needs_input` |
| `success` | `status_done` |
| `text`, `neutral` | `text` |
| `strong` | `weight::SEMIBOLD` |
| `em` | italic |
| `del` | strikethrough |
| `mark` | a highlighted background |
| `mono`, `code`, `path`, `icon` | the terminal font, with `TERMINAL_GLYPH_FALLBACKS` for private-use glyphs, upright |
| anything else (omp's own palette names, `statusLine...`) | `text`, no change |

A node's `tone` maps like this: `neutral` to `text`; `accent`, `info`, `user` to `accent`; `success` to `status_done`; `warning`, `pending` to `status_needs_input`; `error` to `danger_text`; `muted` to `text_muted`; an unknown tone to `text`.
A `gap` is `none` 0, `xs` `space::XS`, `sm` `space::S`, `md` `space::M`, `lg` `space::L`.
A `card` is `surface_raised` with `radius::MEDIUM`.
`spinner`, `elapsed`, `shimmer` and `rate` update on the 500 ms status tick; nothing loops (ADR 0086).
The `rate` label shows one decimal below 10.0 tok/s (`3.3 tok/s`) and none from 10.0 up (`65 tok/s`).
A named `icon` node maps to a Lucide icon: `copy` Copy; `x` X; `eye` Eye; `model`, `wand` Sparkles; `chev` ChevronDown; `jobs`, `context.compaction` Layers; `job` Zap; `history` History; `link` Link; `open` ExternalLink; `rewind` Undo2; `check` Check; `lightbulb` Lightbulb; `loop` RotateCw; `back` ChevronLeft; `users`, `agents` Users; `agent` Bot; `bash` Terminal; `session` SquareTerminal; `time` Clock; `folder` Folder; `plus` Plus; `wrench` Settings; `x-circle` SquareX; `send` Send; `cache`, `input`, `output` (the token facts).
A name with no entry (`eye-off`, `pause`, `gauge`, `corner-down-right`, `list`, `message`, `activity`, ...) draws nothing and is logged once at `debug`.
A kind the UI does not know is drawn generically: the node's text-like props (`text`, `title`, `head`, `label`, `name`) as a muted line, then its children.

The omp question card (§11) draws by node `role`:

| Role | Drawn as |
| --- | --- |
| `omp.ask.questions` (`tabs`) | tab chips; the active one on `card`; an answered tab keeps its leading check |
| `omp.ask.question` | text in `weight::SEMIBOLD` |
| `omp.ask.option`, `omp.ask.other` | a radio circle, label in `weight::MEDIUM`, detail in `text_muted`; the circle is filled with `accent` when `tone` is `accent`; the picked row on `surface_raised` with a 2 px `accent` bar at its left |
| `omp.ask.check`, `omp.ask.other.check` | the same with a check box |
| the option `value` ("Recommended") | a `badge` at the right |
| `omp.ask.preview` | Markdown under the list |
| `omp.ask.notice` | a line; its `warning` span in `status_needs_input` |
| `omp.ask.answers` | the answers table |
| `omp.btn` | a pill button; `tone` `accent` is `accent` with `on_accent` text; the `kbd` child is not drawn, the node's `title` is the tooltip |
| `meter` with `style` `ring` | a small ring with its `label` |
| `omp.editor.ask.hidden` | one row: summary, Skip, Show |

The todo marks (`checklist`, §11) are: an `active` item an `accent` ring around a dot, its text in `accent`; `pending` a `text_muted` ring; `done` a `status_done` check with muted text; `dropped` muted text struck through; `blocked` text in `status_needs_input` followed by " (blocked)"; a phase with an active item the `active` mark, a phase with every item done or dropped a `status_done` filled circle, any other phase a `text_muted` ring; `done/count` in `text_muted`; the HUD popover is a `floating_panel`.

The omp dock (§11) draws by node `role`, from the Chat composer's tokens:

| Role | Drawn as |
| --- | --- |
| `omp.editor` | the composer card: `surface_raised`, `radius::COMPOSER`, glass border, float shadow; padding `space::M` top and right, `space::S` bottom, `space::L` left |
| `omp.composer.context` (`meter`, `style` `bar`) | the context meter: a 1 px (`CONTEXT_METER_HEIGHT`) line, `text_muted` fill, `status_needs_input` past `thresholds.warn`, `danger_text` past `bad`; `label` above the end of the fill and `total` at the right in `text_muted`; marks show while hovered |
| `omp.composer.line` | the editor line in `BODY`; placeholder upright `text_subtle`, no icon; `ghost` in `text_subtle` after the caret |
| `omp.composer.model` | the Chat model pill (1 px `border` ring, no fill, `text` at 500, chevron) |
| `omp.composer.effort`, `omp.composer.usage`, `omp.composer.rate`, `omp.composer.fact` | the effort glyph leads the model pill (label in its tooltip); usage, rate and facts are quiet mono chips in `text_subtle`, icons too (colors omp puts in spans stay; tone warning / error take `status_needs_input` / `danger_text`); an `accent` span takes the permission chip look |
| `omp.composer.send`, `omp.composer.stop` | the round Send (neutral, `text` fill on hover) and Stop (a `text` circle with a square, `text_muted` on hover) |
| `omp.working`, `omp.working.label`, `omp.working.stop` | the working row: dots, elapsed in mono `text_subtle`, divider, label in `text_muted`; `omp.working.stop` is the card's Stop when the bar has none |
| `omp.hud`, `omp.hud.seg` | one pill with a `border` ring and no fill, one padded cell per segment (icon, count in mono, tooltip), a `BORDER_WIDTH` divider in `border` between cells |
| `omp.hud.todo` | the todo HUD chip (§11) |

---

## §15 Non-functional requirements

| Area | Requirement |
| --- | --- |
| UI startup | first frame < 500 ms with the daemon running |
| Input latency | key press → glyph on screen < 16 ms (local), also beside busy panes and in an omp pane with a 2,000-row transcript (ADR 0114) |
| Idle CPU | UI + daemon < 1% of the machine with no output (GPUI's Windows vsync floor alone is 1–2% of one core) |
| Memory | UI < 150 MB, daemon < 50 MB with 6 sessions (agents excluded) |
| Security | IPC endpoint accessible by the current user only; the daemon has no network listener (`mk web` — started by the user, or by the daemon as a child while the user turned phone access on, ADR 0053 — listens on 127.0.0.1 by default and requires its token on every WebSocket, ADR 0052); with `[web] push = true` `mk web` also POSTs encrypted Web Push messages to the push service host (Apple, Google, Mozilla, Microsoft only) of each subscribed phone, ADR 0067; `hook.report` rejected unless the session id belongs to a live session; other computers reach it only through SSH and `mk proxy` (ADR 0089) |
| Data | SQLite WAL; daily DB backup, keep 7 |
| Errors | panics logged with backtrace; UI shows an error instead of dying silently |
| History | per-session output log, rotated at 5 MB; the History dialog replays its newest 1 MiB, built off the UI thread (ADR 0088) |
| Throughput | 6 panes each receiving a continuous 50 MB output flood: UI keeps < 16 ms input latency, RAM stays flat (no unbounded growth), daemon CPU < 1 core |
| Git | status refresh on a 10,000-file repo < 150 ms on Windows |

---

## §16 Testing strategy & fake agent

- **Unit tests:** state machine, profile parsing, frame codec, task transitions, keymap parsing,
  comment re-anchoring, Markdown → render model.
- **Integration tests:** real daemon in a temp dir (`tempfile`), real socket/pipe, sessions running
  `mkrew-fakeagent`.
- **Snapshot tests (`insta`):** ANSI-stripped output, parse errors, terminal grid dumps (feed recorded
  bytes into `alacritty_terminal`, snapshot the resulting text grid).
- **No test may call a real agent or the network.**

**`mkrew-fakeagent`** — a scriptable fake agent. Reads a scenario (TOML/JSON) via
`--scenario <file|name>` or env `MKREW_FAKE_SCENARIO` (an existing file path wins, else a built-in
name: `ask` (default), `flood`, `omp` (spinner turns + quiet gaps, like Oh My Pi)), and can:
print colored output and spinners; ask "Do you want to proceed? (y/n)" and wait for input; call `mk hook fake <event>` like Claude Code would; write/edit
files; run `mk comment reply …`; exit with a given code. Ships `agents/fake.toml` and sample
scenarios in `crates/mkrew-fakeagent/scenarios/`, including a `flood` scenario that prints
high-volume colored output as fast as possible (used by the stress test, §15). Almost every acceptance criterion should be
checkable end-to-end with it.

`--tsp <fixture.jsonl>` with `--record <file>` (ADR 0107) makes it a TSP terminal peer: it sets the console to raw VT input, prints a TSP hello, records every byte it reads, waits up to 10 s for the daemon's reply, then replays the fixture's omp-to-terminal messages and prints `fake tsp done`; with no reply it prints `fake tsp: no reply` and keeps running.
`--hello-delay-ms <n>` prints a line and waits `n` ms before the hello.
The committed fixture is omp 18.13.2's own startup record (`docs/superpowers/specs/2026-11-23-omp-tsp/omp-18.13.2-startup.tsp.jsonl`).

**Automation commands** `mk send` and `mk wait` (§9) are used by e2e tests and by users' scripts.

---

## §17 Conventions & Windows notes

**Code**

- `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings` must be clean.
- No `unwrap()/expect()` outside tests, except where provably infallible (with a comment).
- Libraries use `thiserror`; binaries use `anyhow` with context.
- Small commits that build; Conventional Commits (`feat(daemon): …`, `fix(ui): …`).
- Decisions that deviate from this spec: write `docs/decisions/NNNN-title.md` and update this spec in
  the same commit.

**Windows**

- PTY via ConPTY (`portable-pty`); support Windows 10 1809+ and Windows 11.
- ConPTY is sideloaded (ADR 0107): `conpty.dll` and `OpenConsole.exe` from the `Microsoft.Windows.Console.ConPTY` NuGet package 1.24.260710001 (MIT; nupkg SHA-256 `175640566a3b59c4b132070ee96c2c77e5ab7edd2e92732a5eb3610bbf63d90e`) sit next to `mkrew-daemon.exe`, and `portable-pty` loads that `conpty.dll`.
  Older system ConPTYs drop the APC sequences TSP needs; on Windows 11 24H2 (build 26100) the system ConPTY was observed passing TSP through (2026-11-24), but the sideload stays the supported path.
  `scripts/fetch-conpty.sh` (and `.ps1`) download the pinned package once into a cache, check the SHA-256 of the package and of both files, and copy them into `target/debug` and `target/release`; `scripts/verify.sh`, `scripts/verify.ps1` and `scripts/package-setup.ps1` call it, and `packaging/mkrew.iss` installs both files into `{app}\bin`.
  The first run needs the network to fill the cache (so a Windows verify needs one online run); later runs use the cache.
  The files apply to every PTY the daemon opens (shells, Claude, Codex), not only omp's.
  Without them the daemon runs on the system ConPTY, logs `conpty = "system"` once at start, and an omp pane falls back to the terminal grid 10 s after its first output (the wait is long because resumed and concurrent omp starts need over 2 s to send the hello; a hello that still comes within 60 s of the first output switches the pane to the native surface); with them it logs `conpty = "sideloaded"`.
- Agent commands are often `.cmd`/`.ps1` shims — honor `PATHEXT`.
- Named pipe restricted to the current user.
- Paths: `dunce::canonicalize` (no `\\?\` prefix); case-insensitive comparison on Windows.
- Daemon starts with no console window.
- CI: GitHub Actions on `windows-latest` and `ubuntu-latest` from M1.

---

## §18 Milestones & acceptance criteria

Thirteen milestones, strictly in order: M0–M8 = v1, M9–M11 and M13 = v2 (§20); M12 was dropped (ADR 0013) and its number is not reused. A milestone is done only when all criteria are checked and
`scripts/verify` is green on the current OS (CI covers the other). M0 is a risk spike and the only
**blocking human gate**: if GPUI fails, switch the UI to Tauri before investing further.

### M0 — Terminal-in-GPUI spike

Temporary binary `spike-term`: a GPUI window with one terminal running a shell via `portable-pty`
+ `alacritty_terminal`. Code is later moved into `mkrew-ui` or deleted.

- [ ] Runs `claude` (or `codex`) in the window; TUI renders with correct colors, no broken frames.
- [ ] Typing, arrows, Ctrl+C, multi-line paste work.
- [ ] Window resize redraws the TUI correctly.
- [ ] Works on native Windows.
- [ ] Automated: grid-dump snapshot test fed with recorded agent output.
- [ ] Measure on Windows and record in the ADR: idle RAM of the GPUI window with one terminal,
      key-to-glyph latency, and CPU while `spike-term` renders a high-volume output flood.
- [ ] Record findings + recommendation (GPUI vs Tauri) in `docs/decisions/0001-ui-framework.md`;
      add the visual checks to `docs/HUMAN_CHECKS.md`; set loop status to `WAITING_HUMAN`.

### M1 — Daemon & CLI, no UI

- [ ] `mk daemon start` runs detached; `status`/`stop` work; starting twice never yields two daemons.
- [ ] `mk project add/list` persist to SQLite.
- [ ] Sessions spawn from TOML profiles; `mk attach` is fully interactive; `Ctrl+]` detaches, agent keeps running.
- [ ] Re-attaching restores the screen instantly from the daemon snapshot.
- [ ] Binary output frames + backpressure (§7); PTY reader threads + headless emulator (§8).
- [ ] Broken profile TOML → clear error with file and line; daemon does not crash.
- [ ] `mkrew-fakeagent` + `agents/fake.toml` exist; `mk send` and `mk wait` work with it.
- [ ] E2E test: real daemon in a temp dir, fake agent, `mk send` an answer, `mk wait` until done.
- [ ] CI workflow for Windows + Linux.

### M2 — UI: workspaces & terminals

- [ ] UI spawns the daemon if needed.
- [ ] Workspace sidebar; add workspace via folder picker or by cloning a Git URL (ADR 0076).
- [ ] Many terminal tabs per workspace; split panes vertically/horizontally; agent or shell panes.
- [ ] Tabs and split layout persist across UI restarts and daemon restarts.
- [ ] Workspace/tab/pane/copy/paste/search shortcuts from §13 work; unbound keys reach the agent
      (verify Ctrl+C, Ctrl+R, Esc inside Claude Code).
- [ ] Close and reopen UI: sessions still alive and re-attached.
- [ ] "Open in Zed" works.
- [ ] Design tokens in `theme.rs`; design check from §14 done.
- [ ] UI idle with 3 workspaces, 6 sessions: < 150 MB RAM.
- [ ] Terminal rendering follows the §10 performance rules; stress test (fake agent `flood` in
      6 panes) meets §15 throughput.
- [ ] Default agent per workspace used for new agent panes.

### M3 — Agent state, inbox, notifications

After M3 Mkrew is usable day to day (it can replace a plain terminal multiplexer).

- [ ] `mk integration install claude` and `codex` work: backup, idempotent, uninstall.
- [ ] Claude Code asks for approval → within 2 s the session is needs_input, Inbox has an entry, OS notification fires.
- [ ] Clicking the OS notification brings the app up focused on that pane.
- [ ] Per-session status correct on tab icon and sidebar (§11); the tab tooltip gives the status in words.
- [ ] omp (screen detection) transitions correctly between working and idle.
- [ ] Unit tests for the state machine; e2e tests with the fake agent for every transition.

### M4 — Task board

- [ ] Create/edit/delete tasks; drag between columns; order persists.
- [ ] Run spawns the right agent (task's, else workspace default) in the right folder; prompt delivered per `prompt_mode`.
- [ ] Task status follows session state (§5 transitions): needs_input, running, review on finish.
- [ ] Task card opens its session's terminal.
- [ ] Follow-up input on running cards sends via `session.send`.
- [ ] `mk task add/list/run` share data with the UI.

### M5 — Resume, checkpoints, conflicts, history

- [ ] Stop and restart the daemon: running Claude/Codex tasks resume the right conversation.
- [ ] Before Run in a git repo: store HEAD in `task.checkpoint` (only the project folder's repository counts; child repositories of ADR 0029 get no checkpoint); "Undo task" shows the diff to be
      discarded and only runs `git restore`/`git clean` after explicit confirmation.
- [ ] Two sessions in one project edit the same file within 60 s → `conflict.detected`, UI warns on both cards.
- [ ] Watcher ignores `.git`, `node_modules`, `target` and respects `.gitignore`.
- [ ] Per-session output logs; history of finished sessions viewable after a daemon restart.
- [ ] Daily DB backups, keep 7.

### M6 — Markdown review

- [ ] Docs view lists files by `review_globs` + md files agents just changed; preview renders
      headings, lists, task lists, tables, code blocks correctly.
- [ ] Comment on a selection; threads with reply, resolve, reopen; persisted in SQLite.
- [ ] External edits (e.g. in Zed) refresh the preview and re-anchor comments; deleted passages → `outdated`.
- [ ] "Send to agent" types the prompt into the right session; queued while working.
- [ ] Agent runs `mk comment reply` → reply appears in the thread, comment becomes `addressed`.
- [ ] "View changes" diffs send-time snapshot vs current file.
- [ ] E2E with the fake agent: plan written, 2 comments sent, both edited and answered.

### M7 — Files & Git

- [ ] Files view: lazy tree, respects `.gitignore`, git markers; viewer/editor with syntax
      highlighting; large/binary files handled.
- [ ] The palette's Files scope fuzzy-finds files in a 10,000-file repo in < 100 ms.
- [ ] Git view header: branch, upstream, ahead/behind; Working changes grouped Staged/Unstaged/Untracked.
- [ ] Against base uses `merge-base` correctly; base auto-detects `origin/HEAD` → `main` → `master`
      and is configurable per workspace (UI and `mk project update --base`).
- [ ] Unified diff; stage/unstage per file and per hunk; discard always confirms.
- [ ] Commit from the UI; push current branch to upstream (never force); fetch.
- [ ] History of 50 commits with per-commit diffs.
- [ ] "View changes" on a task card shows the diff since the task checkpoint.
- [ ] Diff-line comments sent to an agent; agent fixes and replies via `mk comment reply`
      (e2e test with the fake agent).
- [ ] Integration tests run real `git` on temp repos (`tempfile`): status, diff, hunk staging,
      commit, merge-base.
- [ ] `GitBackend` trait; status refresh meets §15 on a generated 10,000-file repo.

### M8 — Polish & packaging

- [ ] Full command palette; keymap customization via `keymap.toml` with duplicate detection.
- [ ] Dev server per workspace: run `dev_command` in its own tab; optional auto-start after daemon start.
- [ ] Windows setup .exe (per-user, no admin), macOS app zip, Linux tar.gz.
- [ ] README: install, configure agents, uninstall.
- [ ] Non-functional checks from §15 measured and recorded in `docs/decisions/` or the README.

### M9 — Attention (§20.0, §20A)

- [ ] `settings.toml` (§20.0): defaults when missing, parse errors reported in `daemon.status`
      with the last good settings kept, re-read after the file changes; unit tests.
- [ ] `session.read` returns the last screen lines; the peek popover shows them for a sidebar row,
      and its quick reply answers a fake agent's "(y/n)" without leaving the view (e2e
      for the request + manual check for the popover).
- [ ] Turns start and end as §20A.3 defines; a fake agent that edits 2 files in one turn produces
      `session.turn` with both paths (+/− counts arrive in M11); the tab tooltip shows the summary.
- [ ] `notify.needs_input` / `notify.done` / `notify.sound` switches are respected (unit tests on
      the decision; manual check for the sound).
- [ ] Keep awake: inhibitor held while an agent works and released ≤ 5 s after the last one
      stops (fake inhibitor test); real backends per OS; manual check (`powercfg /requests`).
- [ ] Broadcast sends one text to every checked session; each result is shown.
- [ ] ~~Inbox rows show the last non-blank screen line~~ removed with the Inbox card (ADR 0036).

### M10 — Instructing (§20B)

- [ ] Composer: a multi-line send arrives as one bracketed paste; `@` inserts `file_ref`-formatted
      paths; drafts survive switching panes.
- [ ] Send queue: `session.send` returns `{send_id, queued}`, `session.queue` events, `session.unqueue`;
      e2e with the fake agent: a text queued while working and removed is never typed.
- [ ] Prompt library: samples seeded on first run, variables rendered (unit tests), insertable from
      the palette, the composer and the task dialog.
- [ ] Send selection from the Editor (also a Markdown preview selection), Docs, Git diff and a terminal opens the right session's composer, prefilled.
- [ ] Dropping files types their paths; pasting an image stores it via `attachment.save` and types
      the path; attachments of sessions ended > 7 days ago are deleted.
- [ ] "Run with…" on a Todo/Review card updates the task's agent and runs it.

### M11 — Reviewing per turn (§20C)

- [ ] A turn snapshot is taken at each turn start without changing HEAD, branches, the index or
      any file (real-git test asserts all four); the `hook.report` reply for a turn-start hook waits for it.
- [ ] `turn.list`, `turn.restore_preview`, `turn.restore`: preview lists changed/created/deleted
      files; restore only with `confirm` and a matching fingerprint; e2e with the fake agent over 2 turns.
- [ ] Snapshot on the generated 10,000-file repo < 300 ms on Windows (perf test like §15).
- [ ] Git view: "Changes since turn n" (`since` accepts a turn's tree) and the "Changed by" session filter.
- [ ] Follow in Zed: debouncer unit-tested; manual check with a real agent.

### M13 — Isolation & usage (§20E)

- [ ] Worktree task: Run creates the worktree and `mkrew/…` branch from HEAD, copies
      `.worktreeinclude` entries, runs the agent inside; the main folder's branch, HEAD, index and
      files are unchanged (real-git test). Resume after a daemon restart uses the worktree.
- [ ] The file tree, the Editor and the Git view work on a task worktree; Merge only when both sides are clean and aborts
      cleanly on conflict; Remove keeps an unmerged branch (real-git tests).
- [ ] Fan-out runs 2–5 agents in their own worktrees; Compare and Pick work; e2e with fake profiles.
- [ ] Usage: fixture transcripts in the current Claude and Codex formats give the expected
      input/output/context tokens, read incrementally; shown in the composer, on cards and sidebar.
- [ ] M9–M11 and M13 manual checks listed in HUMAN_CHECKS.

---

## §19 Risks, open questions, backlog

| Risk | Impact | Mitigation |
| --- | --- | --- |
| GPUI sparse docs, API churn, young Windows support | M0/M2 slow or blocked | M0 is a gate; on failure switch UI to Tauri + React + xterm.js, keep daemon/CLI/protocol |
| Home-made terminal rendering bugs (wide unicode, emoji, cursor, selection) | Agent TUIs render wrong | Test with real Claude Code/Codex/omp from M0; study how Zed uses `alacritty_terminal` |
| Claude Code / Codex hook formats change | State less precise | Screen detection fallback; `mk integration status` reports hook version |
| Wrong screen patterns | False needs_input/idle | Patterns live in TOML, fixable without rebuild |
| Daemon crash | All agents die | Logs + resume on restart; consider a separate PTY host process if crashes recur |

**Open questions (owner: user)**

- [x] Name: **Mkrew**, CLI `mk`.
- [x] GPUI straight away vs Tauri first — **GPUI** (M0 gate, ADR 0001).
- [x] Optional per-task worktree — yes, opt-in per task (§20E, M13).
- [ ] Comments on unchanged code files (diff-line comments exist since M7)?
- [ ] License if the repo goes public.

**Backlog after v2:** compact density; `gix` git backend; pull/rebase/branch switching and
conflict resolution in-app; GitHub (PR/issue → task); money
estimates from token usage.

---

## §20 v2: working with agents (M9–M11, M13)

Goal: make the daily loop with agents fast — notice who needs you, answer without switching
views, instruct precisely, review and undo per turn, and isolate parallel work when asked.
Inspired by herdr (peek), Zed's agent panel (queue, follow, per-turn checkpoints, keep awake) and
Orca (worktrees, fan-out, usage). §1 non-goals still hold: no editor, no LLM API calls, no cloud,
no native mobile app (a browser client exists, ADR 0052). Phone push (ADR 0067) is opt-in and
end-to-end encrypted.

Every v2 feature keeps §2: agent differences live in TOML profiles; the daemon owns state; it
works on native Windows; nothing touches the user's branch, index or files without a confirmed
action (ADR 0010).

### §20.0 Shared pieces

**Settings file** `<config>/mkrew/settings.toml` (`Paths::settings_file()`), read by the daemon
at start and re-read after it changes (the daemon watches its own config dir, debounced 300 ms).
Missing file = all defaults. A parse error keeps the last good settings and is reported in
`daemon.status` (`settings_error`) and by the palette like keymap errors. The UI reads it the
same way and re-reads it within 500 ms of a change (fonts apply live, ADR 0024). Palette
`Global: Open settings` (Ctrl+,) opens the Settings dialog (§14, ADR 0073), which writes this file one
key at a time; the UI applies a dialog change at once. Its "Edit settings.toml in Zed" creates the
commented file if missing and opens it in Zed. Unknown keys
are errors (same rule as profiles). Full shape with defaults:

```toml
[notify]
needs_input = true          # OS notification on needs_input
done = true                 # OS notification on done
sound = true                # notification plays the OS default sound
flash_taskbar = true        # §20J; Windows: the taskbar button flashes, macOS: the Dock icon bounces, while a session needs input and the window is not focused

[awake]
keep_awake = true           # §20A.5

[composer]
file_ref = "@{path}"        # §20B.1; how a picked file is typed; "{path}" alone for agents without @

[zed]
follow_debounce_ms = 1500   # §20C.4

[notes]                     # §10, §12A; where New note saves its file (ADR 0108)
folder = ""                 # §10, §20.0; '' = the workspace root, relative = under the root, absolute = as is (not ~; no ..); one folder for every workspace (ADR 0108); the daemon reads it

[ui]
font_family = ""            # §14; "" = system UI font; other UI sizes keep their ratio to it
font_size = 13              # px, 8–32
theme = "system"            # §14; "system" | "light" | "dark"; Ctrl+Shift+J and Settings write it
light_theme = "Mkrew Light" # §14; a built-in theme name or the name of a file in themes/, shown in Light mode / a light OS
dark_theme = "Mkrew Dark"   # §14; the same for Dark mode / a dark OS
diff_mode = "unified"       # §12B; "unified" | "split"; the Git diff header toggle writes it
inspector_width = 300       # §10; px, the right panel's width (dragging its edge writes it; kept 240–560)
right_panel = "hidden"      # §10; "files" | "session" | "git" | "hidden"; the right panel's toggle and tabs write it
sidebar_width = 280         # §10; px, the workspace sidebar's width (dragging its edge writes it; kept 200–480)
close_to_tray = true        # §20J, §20L; Windows: ✕ of the last Space window hides every window to the tray icon; macOS: ✕ hides the app (false: ✕ quits the UI)
window_mode = "single"      # §10 Window modes; "single" (one window, sidebar) | "per_workspace" (a window per workspace, no sidebar)
open_workspace = "ask"      # §10 Window modes; per_workspace only: opening a workspace without a window "ask" | "new_window" | "replace"
panel_side = "right"        # §10; "right" | "left": the Files / Session / Git panel's side of the panes
board_all_workspaces = false # §10; the Board shows every workspace's tasks; its `This workspace | All workspaces` switch writes it
md_mode = "preview"         # §12A; preview | split | source: how the next .md file opens; switching a file's mode writes it
md_split = 50               # §12A; the source's share of the Markdown Split view, percent (kept 20-80); dragging the divider writes it
chat_default = false         # §11; agent panes without a saved mode open in Chat (Claude, Codex, omp); each pane's Terminal | Chat switch overrides it and is also written here as the last pick
# widget_bar, widgets_hidden: retired (ADR 0106). A file that still has them loads; they are ignored and never written.

[terminal]
font_family = ""            # §14; "" = first installed of the platform list; use a monospace family
font_size = 13              # px, 8–32; also the Editor and Git code views
option_as_meta = false      # macOS: true = Option sends Esc + the key (Meta); false = Option types the layout's characters (ADR 0099)

[voice]                     # §20G, ADR 0045
base_url = "https://api.groq.com/openai/v1"  # OpenAI-compatible; POST {base_url}/audio/transcriptions
model = "whisper-large-v3-turbo"
api_key_env = "GROQ_API_KEY"  # env var holding the key ("" = no key, local server); never the key itself
language = ""               # "" = detected per recording; "vi" | "en" forces one
prompt = "Mkrew, Claude Code, Codex, commit, branch, merge, refactor, test, bug. Sửa lỗi này rồi chạy test lại nhé."
max_seconds = 120           # 1–600; a recording stops by itself after this
[features]                  # ADR 0046; optional modules, off by default
docs = false                # §12 Docs view, "Review in Docs"; applies live
turns = false               # §20C.1–2 turn snapshots + "Turns…"; read when a session starts
share = false               # ADR 0106; the pane's Share button (title bar toggle, Follow in Zed); applies live
[web]                       # ADR 0053; phone access
enabled = false             # the daemon runs `mk web` while true; the Phone access dialog writes it
push = false                # ADR 0067; mk web notifies phones that opted in (sends while `mk web` runs); the Phone access dialog writes it
[jira]                      # §20I, ADR 0066; "" site = off
site = ""                   # "acme.atlassian.net"
email = ""
api_token_env = "JIRA_API_TOKEN"  # env var holding the API token; never the token itself
mine = false                      # the Board's All | Mine filter (it writes this)
layout = "list"                   # the Board's List | Kanban switch (it writes this)
```

**Phone access** (ADR 0053). While `[web] enabled = true` the daemon supervises `mk web` as a
windowless child (`mk.exe` next to `mkrew-daemon.exe`, else on PATH; stderr's last line is the
`error`; a child that exits is started again after 1, 2, 4 … 60 s; `enabled = false` kills it).
`daemon.status.web` reports `{enabled, running, error?}`. The Phone access dialog (§14) writes the
key and shows the link; `mk web --new-token` still revokes old links.

**Phone notifications** (ADR 0067). While `[web] push = true`, `mk web` watches `session.state` on its
own daemon connection and, for each session entering `needs_input` or `done` (subject to
`[notify] needs_input` / `done` and one per session per 30 s), sends a Web Push message
(RFC 8291 `aes128gcm`, VAPID RFC 8292) to every phone that tapped the bell on the page. Title
`<session> · <workspace>`, body `needs you` / `done`; tapping it opens the page on that session.
No notification payload carries the access token; the manifest `start_url` does (the iOS Home
Screen app stores it). Subscriptions and the VAPID key live in `<config>/web_push.json`; a push
service answer 404 / 410 removes a subscription; `mk web --new-token` empties the list. Only Apple,
Google, Mozilla and Microsoft push hosts are accepted. The page needs HTTPS (`tailscale serve`), and
on iPhone iOS 16.4+ with the page added to the Home Screen.

**Turn.** One request→answer cycle of an agent session. A turn **starts** when the session
enters `working` from `starting`, `idle`, `done` or `needs_input` after user input; it **ends** at
the next `done`, `idle`, `needs_input` or `exited`. A `needs_input` → `working` resume (approval)
continues the same turn. Turns are numbered per session from 1. Pure model in
`mkrew_core::turn` (unit-tested on state sequences); the daemon's actor owns one per session.

**Profile keys added in v2** (all optional; unknown keys stay errors):

```toml
[usage]                        # §20E.3
format = "claude_jsonl"        # "claude_jsonl" | "codex_jsonl" | "omp_jsonl"; absent = no usage
context_window = 200000        # tokens, for the % shown
```

`tsp` (bool, §6, ADR 0107) is also an optional top-level profile key; absent means on for a profile whose `[usage] format` is `omp_jsonl`.

### §20A Attention (M9)

1. **Peek + quick reply.** Hovering a sidebar session row for 300 ms (full-width sidebar; the compact rail shows a tooltip instead, ADR 0040), or `Ctrl+Shift+Space`
   (first session that needs you; ↑/↓ cycles those sessions, Esc closes; the row is shown first: its workspace opens and the filter clears), opens a popover with the last 8 non-blank
   screen lines (monospace plain text, ADR 0012), the state and its age, and a one-line input. Enter sends
   the text with submit (`session.send`); for a `needs_input` session the popover also shows
   buttons `y` and `n` (Alt+Y / Alt+N) that send that letter with submit. Ctrl+O opens the session's pane and
   closes the popover. After a send, a popover with the keyboard moves on to the next session that needs
   you (it closes when there is none). Focus returns where it was. Data comes
   from the new request `session.read` (below), never from attaching.
2. **Sound.** OS notifications (§8) play the OS default sound when `notify.sound` (notify-rust
   `sound_name` on macOS, a hint on Linux; the toast's `<audio>` on Windows). `notify.needs_input` / `notify.done` switch each kind
   off. The 30 s per-session limit stays.
3. **Turn summary.** When a turn ends the daemon broadcasts `session.turn` with `n`, `started_at`,
   `ended_at` and `files` = paths attributed to the session during the turn (the hook-reported
   edits and file-watcher batches already attributed in M5, deduplicated, relative). From M11 each
   file also carries `added`/`removed` (numstat between the turn's start and end snapshots).
   The tab tooltip of a `done` session shows `Done · 3 files · 4m` (no files → `Done · 4m`;
   durations as everywhere else, ADR 0011). The summary text is UI-side; the event carries the data.
4. **Last line.** (Removed with the Inbox card, ADR 0036: no list shows it any more; the peek popover shows the last 8 lines.)
5. **Keep awake.** While at least one agent session (not shell) is `working` and
   `awake.keep_awake`, the daemon holds a sleep inhibitor; it releases it 5 s after the last one
   leaves `working`. Display sleep and screen lock stay allowed. Windows:
   `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)` on a dedicated thread (the flag is
   per thread); macOS: child `caffeinate -i -w <daemon pid>`; Linux: child
   `systemd-inhibit --what=idle:sleep --why=Mkrew --mode=block sleep infinity` (missing tool → log
   once, continue). Behind a small `Inhibitor` trait so tests use a fake.
6. **Broadcast.** Palette command "Broadcast to agents…": a dialog lists the workspace's live agent
   sessions with checkboxes (all checked), a text input, Send = one `session.send` per checked
   session (submit). Each row shows sent / queued / failed. No daemon request.

**Protocol (M9):**

| Method / event | Params | Returns / data |
| --- | --- | --- |
| `session.read` | `session_id`, `lines` (1–200, default 20) | `lines[]` (plain text, last non-blank rows of the emulator screen, top to bottom), `state`, `state_changed_at` — works for live sessions only (`invalid_state` otherwise) |
| event `session.turn` | – | `session_id`, `n`, `started_at`, `ended_at`, `files[]` (`path`, `added?`, `removed?`) |

`daemon.status` gains `settings_error?`.

### §20B Instructing (M10)

1. **Prompt composer.** `Ctrl+Shift+Enter` on a focused agent pane opens a composer docked at the
   pane's bottom: a multi-line text box (Enter = newline, Ctrl+Enter = send, Esc = hide and keep
   the draft; these keys are local to the composer like dialog keys, so they never reach the
   terminal or Run task).
   Send = `session.send` with submit; multi-line text already goes as one bracketed paste (§7).
   Typing `@` or `/` at the start of a line or after whitespace opens a picker; in the middle of a word (`a@b.com`, `src/main.rs`) they type as they are.
   `@` opens the palette on its Files scope in insert mode (§13); the pick inserts `composer.file_ref` with `{path}` = the project-relative path, `/` separators, quoted with `"` if it contains spaces.
   `/` opens the agent's command picker (§20B.7).
   Drafts are kept per session in UI memory (lost on UI restart, which is fine).
   In a pane in Chat mode (§11) the composer is always shown: Enter sends, Shift+Enter is a new line, Esc moves the keys to the chat list instead of hiding it (ADR 0104).
   In Chat the composer is a floating card (ADR 0105, ADR 0116): a 1 px context meter above it (`text_muted` fill, the percent above the end of the fill and the window at the right; the warning color from `WARNING_PERCENT`; hidden without a window), the field with the session title in quotes as its placeholder (`text_subtle`, upright, no icon), and a chip row: the model as a ring pill (a click switches the pane to Terminal and types `/model` + Enter, so the agent's own picker opens), the permission chip with a lightning icon, the effort chip, the branch chip (only at the workspace root, `name *N` changed files), then the context tokens (`74K`, database icon), `102 tok/s`, the elapsed time and the local time the last turn ended (`09:44:27`, clock icons) in the terminal font, all quiet in `text_subtle`, or the status line while it shows, ✦, "Prompts…" and a round send button (a neutral fill that turns to `text` on hover, never accent; dimmed while the draft is blank or busy) whose tooltip holds the key hint.
   A narrow pane drops chips from the right: the end time, stats, permission, effort, branch; the model chip and the send button stay.
   When the chat list is scrolled up, a small round down-arrow button floats above the composer at the column's right edge and scrolls to the end.
   While the pane has no keyboard and the draft, the queue and the error are empty, the card is a one-line pill with the placeholder; a click opens it.
   In the `/` picker `/model` shows the current model and `/compact` the context share.
   ✦ (shown while the composer has the keyboard and the draft is not blank) sends the draft to
   the session's own agent profile via `ai.generate` (`enhance_prompt`, §7). The agent may read
   the project folder for context. The rewrite replaces the draft, and the user still sends it.
   Typing waits until the answer arrives; picked prompts or files, selections and pastes made
   meanwhile are added after it. A failure keeps the draft and shows why.
   The task dialog's prompt (Board "New task", edit task) is edited the same way: `@` picks a file, `/` an agent command,
   "From prompt…" inserts a library prompt, and ✦ "Improve" rewrites it with the task's
   agent (the workspace default when none is picked), with the same waiting and failure rules;
   saving waits for the answer.
   Ctrl+V with an image saves it as a project attachment (§20B.5) and types its path, as in the composer.
   OS files dropped on the Prompt box type their absolute paths as on a pane (§20B.5).
   The Board follow-up input reuses the same multi-line text box.
2. **Send queue visible.** While a send waits (session working), the composer lists queued texts
   (first line each) with ✕ to remove. Daemon: each queued text gets a `send_id`; `session.send`
   returns `{send_id, queued: bool}`; `session.unqueue` removes a not-yet-written text (`not_found`
   once written); event `session.queue` carries the session's current queue after every change
   (an exiting session announces it empty). A floating badge in the pane shows `⧗ 2 queued`.
3. **Prompt library.** Markdown files in `<config>/mkrew/prompts/*.md`; file stem = name; optional
   first line `# Title`. Variables: `{file}` (current file of the Editor/Git, project-relative),
   `{selection}` (picked lines, as quoted text), `{task}` (task title), `{project}` (name). An unknown
   or unavailable variable renders as empty and is listed in a muted note under the preview. Pure
   `mkrew_core::prompt::render(template, vars)`. Entry points: palette "Insert prompt…" (into the
   open composer, else opens one on the focused agent pane), a "Prompts" menu in the composer,
   and a "From prompt…" menu in the task dialog (one modal picker).
   Three samples are seeded on first run if the dir is missing: `review.md`, `explain.md`, `fix-tests.md`.
   Read and written by the UI directly (config, not state); a machine window uses this computer's folder.
   The picker's last row "Manage prompts…" and the palette's "Global: Manage prompts…" open Settings → Prompts (§14), where prompts are added, edited, renamed and deleted with `mkrew_core::prompt::{validate_name, create, save, rename, delete, read}`.
   A name is at most 64 characters and cannot be empty, start with a dot, contain `/ \ : * ? " < > |` or a control character, end in a space or a dot, or be a Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, also before a first dot); it is unique ignoring case, except that a prompt may be renamed to a new case of its own name.
   `save` writes a temp file in the folder and renames it over the target.
4. **Send selection to agent.** `Ctrl+Shift+L` in a file pane (a Markdown preview's selection as its blocks' source lines; else the editor's selected lines, else the caret's line), Docs (the comment selection), Git diff (picked new-side
   lines) and a terminal (selected text) opens a session picker (default: the task's session for
   Git task changes, else the workspace's most recently active agent session; same list as
   §12's send dialog), then that session's composer prefilled with
   `` `<path>:<a>-<b>` `` + a fenced block of the text (terminal: fenced block only). The user edits
   and sends.
5. **Drop files / paste images.** Dropping files from the OS onto an agent or shell pane (gpui
   `ExternalPaths`) types their absolute paths, each quoted with `"` if needed, separated by spaces,
   with no Enter. The whole pane accepts the drop (terminal, composer, and the drag handle at the top). While the pane's
   composer is shown, the paths go into its draft instead of the terminal. `Ctrl+V` /
   `Ctrl+Shift+V` with an image on the clipboard (gpui `ClipboardEntry::Image`; a BMP image is
   re-encoded as PNG) saves it through `attachment.save` and types the returned path the same
   way; copied files type their paths. Text clipboard behaves as before.
   **Drag from the file tree** (ADR 0108).
   Dragging a row of the file tree (§12A) onto an agent pane's composer inserts `composer.file_ref` with the path relative to the tree's folder (the project, or the task worktree); a folder inserts its path ending with `/`, then one space; no Enter.
   A composer that is not shown takes no drop, and a row dropped on a terminal types nothing.
   The composer shows an accent ring while a row hovers it.
   A file row dropped on a pane (terminal, file or diff) opens the file there, at the edge under the pointer: the pane splits and the new file pane takes that side (§10 "Drops").
   A folder row is accepted only by a composer.
   **Drops on file and diff panes.**
   OS files dropped on a file or diff pane open each file that is inside the workspace root or the notes folder as a new tab.
   Folders and files outside both are skipped, and one error toast says "Only files inside the workspace or the notes folder open in Mkrew".
   A terminal pane keeps typing the dropped paths, as above.
   **Task dialog.** The Prompt box of the task dialog takes the same inputs (ADR 0095).
   A pasted image is saved with `attachment.save` and owner `project` (the dialog's project), because a new task has no id and no session yet.
   Dropped OS files type their absolute paths as on a pane, quoted and separated the same way.
   While a drag is over the box it shows the accent ring (accent border plus a faint tint).
   A machine window refuses OS files as on a pane.
   File tree rows cannot be dragged onto the box: the dialog's modal shade covers the tree, so no drag can start.
   `type_into_pane` keeps ignoring drops while a dialog is open: the box handles its own.
6. **Run with…** A task card's menu offers "Run with…" (pick a profile): updates the task's agent,
   then runs it. Only for Todo or Review cards without a live session.
7. **Slash picker.** Typing `/` at the start of a line or after whitespace in the composer or the task dialog's Prompt opens a picker that lists what typing `/` lists in the agent's own terminal, with the same names, descriptions and order (ADR 0094).
   The daemon builds the list on the machine that runs the agent (`slash.list`, §7): the CLI's built-in commands from a table pinned to a CLI version (`crates/mkrew-core/slash/<cli>.toml`), plus the skills, command files and plugin commands that CLI finds on disk by its documented rules.
   The CLI is never started.
   Which CLI an agent runs comes from the profile's `cli` (§6); a profile with no known CLI shows a note and a button that opens its file.
   The folder scanned is the session's project (its task worktree when the task has one) for a composer, the project folder for a new task, and the task's worktree (else the project folder) for an edited task.
   The agent is the session's profile in a composer, the dialog's Agent (else the workspace default) in the task dialog; with none, `/` types as it is.
   The last answer per (agent, project, worktree) is cached for the connection: reopening shows it at once and refreshes it.
   An empty filter lists the rows in the CLI's order; a filter ranks by fuzzy score on the name, ties in the CLI's order.
   Each row shows `/name`, its argument hint, a one-line description and a source chip (`built-in`, `project`, `user`, `managed`, `plugin: <name>`, `profile`).
   ↑ ↓ move, Enter or Tab types `/name ` into the box, Esc types `/` and what was typed after it.
   In omp the `skill:` row stands for its skills and expands when the filter starts with `skill:`; a group row types nothing.
   Not listed: MCP prompts (the footer says so for Claude), Codex skills (its `$` popup), omp extension and TypeScript commands (add them with `[[slash.add]]`), rows that depend on the user's plan or login (hide them with `[slash] hide`).
   Files that cannot be read are counted in the footer, with the list in a tooltip.

**Protocol (M10):**

| Method / event | Params | Returns / data |
| --- | --- | --- |
| `session.send` (changed) | as before | `send_id`, `queued` |
| `session.unqueue` | `session_id`, `send_id` | ok |
| event `session.queue` | – | `session_id`, `items[]` (`send_id`, `text`) |
| `attachment.save` | `owner` (`{"session": id}` or `{"project": id}`), `ext` (`png`/`jpg`/`gif`/`webp`), `data` (base64, ≤ 10 MiB) | `path`: `<data>/mkrew/attachments/<session_id>/<ulid>.<ext>`, or `<data>/mkrew/attachments/projects/<project_id>/<ulid>.<ext>` for a project; an unknown session or project is `not_found` |

Attachments of sessions that ended more than 7 days ago are deleted by the hourly backup check (§8).
A project attachment is deleted by the same check when it is more than 7 days old (counted from the ULID in its file name) and no task of that project has a prompt containing its file name unless that task has been done for more than 7 days.

### §20C Reviewing per turn (M11)

Snapshots and "Turns…" need `[features] turns = true` (§20.0, ADR 0046); off, turns are recorded
without trees, like outside git.

1. **Turn snapshots.** At each turn start (§20.0) in a git repo (the project folder's repository
   only; child repositories of ADR 0029 are not snapshotted), the daemon records a tree of the
   whole working state without touching the user's index, HEAD or branches: copy `.git/index` to
   a temp file, then with `GIT_INDEX_FILE=<temp>` run `git add -A` and `git write-tree`, delete the
   temp index. Untracked-but-ignored files are not included. The tree stays reachable through
   `refs/mkrew/turns/<session_id>/<n>` (`git update-ref`, a ref outside `refs/heads`, so no branch);
   refs of sessions ended > 7 days ago are deleted at startup. A turn's **end snapshot** is the next
   turn's start snapshot, or one taken when the turn ends with the session done/idle/exited.
   Snapshots run in `spawn_blocking`, one at a time per project (later requests wait). For a hook
   turn start (`UserPromptSubmit`) the `hook.report` reply is sent after the snapshot, so the agent
   has not edited yet; screen-detected starts are best effort (the snapshot may include the first
   edits — documented in ADR 0010).
2. **Restore to before turn n.** The tab menu's "Turns…" lists the session's turns (n, time,
   files, +/−). "Restore before turn n" opens a preview like task undo (§5, ADR 0005):
   files to restore/delete and the diff, then confirm. Restore = `git read-tree` into a temp index +
   `git checkout-index -f` of the paths under the project folder that differ from the start tree
   (ADR 0014), and deletes files created since the snapshot
   that the preview listed (still new). Refused while the session is `working`. The user's real
   index is untouched. `turn.restore` uses the same fingerprint check as `task.undo`.
3. **Diff per turn.** The Git view gains a mode chooser entry "Since turn…" (from the pane menu or
   the Turns dialog): base mode with `since` = the turn's start tree (git diffs between a tree and
   the working tree work like a commit). Header shows `Since turn 3 of claude #2`.
4. **Changed by.** The Git view's file list gains a filter "Changed by: All | <session>…" listing
   sessions that have `files_changed`/hook edits in this project in the last 24 h; it keeps only
   files attributed to that session (data: `session.files`, below).
5. **Follow in Zed.** The tab menu's "Follow in Zed" (also a palette command, and the `Share` toggle in the title bar with `[features] share`). While on, each
   hook-reported edit by that session runs `zed <path>:<line?>` (line from the hook payload when
   present), debounced by `zed.follow_debounce_ms` (the last path wins). Off by default; resets on
   UI restart. UI-side; it reacts to the `session.edited` event.

**Protocol (M11):**

| Method / event | Params | Returns / data |
| --- | --- | --- |
| `turn.list` | `session_id` | `Turn[]`: `n`, `started_at`, `ended_at?`, `start_tree?` (absent outside git), `end_tree?`, `files[]` |
| `turn.restore_preview` | `session_id`, `n` | like `task.undo_preview` (checkpoint = `start_tree`) |
| `turn.restore` | `session_id`, `n`, `confirm: true`, `created[]`, `fingerprint` | ok — rules as `task.undo` |
| `session.files` | `project_id`, `since_ms` | `{session_id → paths[]}` |
| `git.status` / `git.diff` (changed) | `since` also accepts a tree id | as before |
| event `session.edited` | – | `session_id`, `path` (project-relative), `line?` — sent for every hook-reported edit |

Migration `0008_turns.sql`:

```sql
CREATE TABLE turn (
  session_id TEXT NOT NULL REFERENCES session(id),
  n          INTEGER NOT NULL,
  started_at INTEGER NOT NULL,
  ended_at   INTEGER,
  start_tree TEXT,            -- NULL outside a git repo
  end_tree   TEXT,
  files      TEXT NOT NULL DEFAULT '[]',  -- JSON [{path, added?, removed?}]
  PRIMARY KEY (session_id, n)
);
```

Turns are recorded from M9 in memory; M11 persists them (M9's `session.turn` does not need the table).

### §20D Leaving agents running — dropped

Removed with M12 (ADR 0013): webhook, rate-limit auto-continue, task chains, agent-driven CLI
(`mk session read/spawn --prompt`, `mk --skill`, task `created_by`). Not planned; section letters
and milestone numbers are not reused.

### §20E Isolation & usage (M13)

1. **Task worktrees (opt-in).** The task dialog gets "Run in a worktree" (default off; per task).
   On Run in a git repo: `git worktree add -b mkrew/<slug>-<short ulid> <data>/mkrew/worktrees/<project_id>/<task_id> HEAD`
   (slug = task title, ASCII lowercase, `-` separated, ≤ 30 chars). The user's checkout, branch,
   index and files stay untouched. Then every path in `<repo>/.worktreeinclude` (one literal
   relative path per line, `#` comments; must exist and be git-ignored, else skipped with a warning
   in the session's first output line) is copied into the worktree. The session runs with `cwd` =
   the worktree (the project's subfolder of it when the project is a repo subfolder);
   `task.checkpoint` = HEAD as before. Resume uses the same folder. Outside a git repo the task
   runs in the project folder, saying so in the first output line (ADR 0010). Child repositories
   (ADR 0029) are not isolated: a task worktree holds the project folder's repository only, and
   the Git view's repo chooser is hidden while a worktree is shown.
   - The task card shows `⎇ mkrew/<branch>`. The file tree and the Git view gain a target chooser
     "Project | <task worktree>…" — all §12A/§12B features work on a worktree (the daemon resolves
     `project_id` + `worktree_task_id?` to a folder).
   - **Merge back** (card menu, Review/Done cards): allowed only when the worktree has no
     uncommitted changes and the main checkout has none either; the user must commit inside the
     worktree first (Git view on the worktree). Runs `git merge --no-ff --no-edit <branch>` in
     the main checkout after a confirmation that shows the commit list; on conflict runs
     `git merge --abort` and reports the conflicting files. Never on a detached HEAD.
   - **Remove worktree** (card menu, confirm): `git worktree remove` (refused with uncommitted
     changes — the dialog shows them, discard is a separate confirmed step), then
     `git branch -d` (never `-D`; an unmerged branch is kept and named in the result).
   - Deleting a task with a worktree requires removing the worktree first.
2. **Fan-out & compare.** Task menu "Run with several agents…" (pick 2–5 profiles): creates one
   child task per profile (`fanout_of` = the original, same title + ` · <agent>`, worktree on), runs
   them, and moves the original to Running as a group card showing its children's states. When all
   children are in Review, "Compare" opens a view: one column per child with files changed, +/−,
   run time, and a button to open its Git view. "Pick" on one child = Merge back that child
   (rules above) and offers to remove the others' worktrees; the original moves to Review.
3. **Usage.** For profiles with `[usage]`, the daemon finds the session's transcript file by
   `native_session_id` — `claude_jsonl`: `<CLAUDE_CONFIG_DIR or ~/.claude>/projects/*/<id>.jsonl`;
   `codex_jsonl`: `<CODEX_HOME or ~/.codex>/sessions/**/*<id>*.jsonl`; `omp_jsonl`:
   `<PI_CODING_AGENT_DIR or ~/.omp/agent>/sessions/**/*<id>*.jsonl` (the file, never the
   sub-agent folder next to it: omp totals leave out sub-agent tokens) — and reads only appended
   bytes on each turn end (no file watching, nothing while a turn runs). It sums the tokens of the
   session and takes the context size and the model name of the newest message (field names
   checked against the CURRENT formats when implementing; fixtures recorded from real files).
   The counts add up (ADR 0035): `input` = fresh input, cache excluded (Claude
   `input_tokens`; Codex `input_tokens` − `cached_input_tokens`); `cache_read` (Claude
   `cache_read_input_tokens`; Codex `cached_input_tokens`); `cache_write` (Claude
   `cache_creation_input_tokens`; Codex has none, 0); `output`. Total tokens = `input` +
   `cache_read` + `cache_write` + `output`. Context = Claude: `input_tokens` + both cache counts of
   the newest non-sidechain message; Codex: `last_token_usage.total_tokens`. `model` = Claude
   `message.model` of the newest non-sidechain, non-`<synthetic>` assistant line; Codex `model` of
   the newest `turn_context` line. Event `session.usage` (`input`, `output`, `cache_read`,
   `cache_write`, `context`, `context_window?`, `model?`, `transcript?` (the located transcript file's absolute path, sent again when it changes)); the composer shows `ctx 62%` (warning
   color ≥ 80 %), task cards show total tokens (the workspace row shows a session summary, ADR 0036). No money estimate.
   omp (per API call, summed): `message.usage` `input` (already fresh), `cacheRead`, `cacheWrite`,
   `output` of `type: "message"` assistant lines; context = `input` + `cacheRead` + `cacheWrite` of the
   newest call; `model` = `message.model` of the newest call (`model_change` lines are not
   trusted). A call with no tokens (aborted) changes neither. omp's file does not hold the
   context window: `context_window` in the profile is the only source.
   Unknown/changed format → no usage, one warning in the log.
4. **Agent limits (ADR 0102).** Limits belong to the account, so the daemon keeps the newest `AgentLimits` per agent in memory (`observed_at` wins, a tie goes to the later report) and announces each change as `agent.limits`; `limits.get` returns them.
   Codex: `UsageReader` reads `payload.rate_limits` of each `token_count` line (also while `info` is null): `primary` / `secondary` with `window_minutes` 300 are the 5-hour window and 10080 the weekly one, any other length is ignored, `used_percent` and `resets_at` are taken as written, `observed_at` is the line's timestamp; the `track` task passes them on at each turn end.
   Claude: `mk statusline` reads Claude Code's status line JSON from stdin (at most 1 MB), takes `rate_limits.five_hour` and `rate_limits.seven_day` (`used_percentage`, `resets_at` in Unix seconds) and sends `limits.report` within 300 ms; a failure is silent.
   It then runs the user's own status line, saved by `mk integration install claude` in `claude-statusline.json` next to `settings.json`, through the shell with the same stdin and a 5 s timeout, and prints its output; any failure prints nothing and exits 0.
   Install sets `statusLine` to the `mk` path followed by `statusline`, in a form both Git Bash and PowerShell parse (see ADR 0102); uninstall puts the saved line back or removes the key.
   `mk integration status claude` says "partly installed" when the hooks are in but the status line is not.
   omp and Copilot record no limits.

**Protocol (M13):** `task.create/update` accept `worktree: bool`; Task carries `worktree`,
`worktree_path?`, `branch?`, `fanout_of?`; new `task.merge` (`task_id`, `confirm: true`) →
`merged: bool`, `conflicts[]`; `task.remove_worktree` (`task_id`, `confirm: true`) → `branch_kept: bool`;
`task.fanout` (`task_id`, `agent_profiles[]`, `cols`/`rows` default 120×30 for the children's
terminals) → child Task[] (git repos only; if a child fails to start the call errors naming it,
the others keep running and the original moves to Running if any started; the original itself
can no longer be run); `fs.*` and `git.*` accept
`worktree_task_id?` (not `git.set_base`: the base is per project) and `git.changed` carries it;
event `session.usage`; request `session.usage` (`project_id`) → the newest usage of each session
of the project (kept as `session_event` kind `usage`, so totals survive reconnects and restarts).
Since protocol 33 `input` excludes the cache and `cache_read`, `cache_write` and `model?` are
new; an event stored before it reads back with both cache fields 0, no model and its old `input`
(cache included) until the session's next turn end.

Migration `0009_worktrees.sql`:

```sql
ALTER TABLE task ADD COLUMN worktree INTEGER NOT NULL DEFAULT 0;
ALTER TABLE task ADD COLUMN worktree_path TEXT;
ALTER TABLE task ADD COLUMN branch TEXT;
ALTER TABLE task ADD COLUMN fanout_of TEXT REFERENCES task(id);
```

### §20F Testing v2

- `mkrew-fakeagent` gains steps: `edit_files { paths = [...] }` (writes files and reports
  `PostToolUse` hooks), and honors `--prompt`/positional prompt as today.
- Real-git tests (temp repos) for snapshots, restore, worktree add/merge/remove, always asserting
  that HEAD, branch, index and untouched files are unchanged.
- No test calls the network or a real agent: keep-awake through the fake `Inhibitor`, usage
  through fixture transcripts.

### §20G Voice input (outside the milestones, ADR 0045)

Speak instead of typing, in Vietnamese or English (mixed and accented speech included).

1. **Start / stop.** `Voice input` (Ctrl+Shift+D, palette "Global: Voice input (start / stop)")
   records the default microphone, and the same key stops it. It works everywhere, also while a
   terminal, a text field or a dialog has the keyboard. A recording stops by itself after
   `[voice] max_seconds` (§20.0). While transcribing, the key does nothing.
2. **Recording pill.** While recording, a pill floats centered at the bottom of the window
   content. It fades and slides in over `theme::motion::SLOW`. It holds a steady dot, a
   seven-bar waveform, `m:ss`, and `Ctrl+Shift+D stops`. The waveform follows the microphone
   loudness, smoothed: quick up, slow down. A wave runs across the bars, and in silence they keep a
   low breath, so a live microphone never looks still. It is redrawn every
   `theme::VOICE_METER_TICK`. While transcribing, the pill shows the loading dots and
   `Transcribing`, and after that it disappears. It never takes a click or the keyboard.
   The field or terminal pane that has the keyboard gets an accent ring over its border. The ring
   breathes while recording and holds steady while transcribing. Under reduce motion, the ring
   and the wave stand still, and the bars still follow the loudness.
   The sidebar footer (the title bar while the sidebar is hidden, and the rail) shows only the
   steady dot or the loading dots. Errors and notes (`✕ Voice: …`) show there and stay until
   the next start.
3. **Speech to text.** One `POST {base_url}/audio/transcriptions` (OpenAI-compatible: Groq by
   default, OpenAI, or a local server) carries the recording as 16-bit mono WAV in multipart form
   data, with `model`, `response_format=json`, `temperature=0`, and `language` / `prompt` when set.
   The timeout is 60 s. The key is read from the environment variable that `api_key_env` names
   (never from `settings.toml`) and sent as `Authorization: Bearer`; an empty `api_key_env` sends
   no key. Recordings under 0.3 s, or with less than 0.15 s of sound above 1 % of full scale (RMS
   over 30 ms), are not sent ("too short", "heard nothing"), because Whisper invents text for
   silence, and the shortcut's own key click must not count as speech. API errors show the API's
   `error.message`.
4. **Where the text goes.** Whitespace collapses to single spaces, because a line break would be
   Enter and send the text to an agent. When the text arrives, it is typed into the text field
   that has the keyboard exactly like an IME commit (every field of §13 and the Editor). If
   a terminal pane has focus instead, it is pasted there like a paste (bracketed when the program
   asked for it, never followed by Enter). If neither has focus, it goes to the clipboard and the
   footer says so.
5. Nothing is recorded or sent unless the user starts a recording, and audio is never stored.

### §20H Ask AI (outside the milestones, ADR 0061)

Ask an agent any question that is not about a project, without opening a terminal.

1. **Open.** `Ask AI` (Ctrl+Shift+G, palette "Global: Ask AI…") opens a modal dialog, also
   without a workspace.
2. **Agent.** A chip per installed agent profile that can answer (`ask` or `oneshot`, §6);
   the active workspace's default agent is picked first, else the first one. None: a line
   says what to add to a profile.
3. **Web search.** Off / On chips, shown when the picked agent has `ask_web` (else a line names
   the key to add). The choice is kept in memory for the next question.
4. **Question.** A multi-line field: Enter = new line, Ctrl+Enter or "Ask" sends
   `ai.generate` `ask` (§7), Esc or Close closes. One question at a time: "Asking…" and
   `◌ Thinking…` while waiting; no history, each question stands alone.
5. **Answer.** Rendered as Markdown like the Docs preview (scrolls past
   `theme::ASK_ANSWER_HEIGHT`); "Copy answer" puts the Markdown on the clipboard. A failure
   shows `✕` and the reason. The question stays, so it can be edited and asked again.

### §20I Jira on the Board (outside the milestones, ADR 0066)

Pick a Jira ticket on the Board and start a task from it. Jira Cloud only (`*.atlassian.net`,
email + API token); the UI calls Jira's REST API v3 itself, the daemon never does (§1: with
speech to text, this is the second user-configured use of the network, and only when
`[jira] site` is set).

1. **Switch.** With `[jira] site` set (§20.0), a `Tasks | Jira` switch heads the Board; an empty
   `site` shows no switch (a hand edit of `settings.toml` applies within about a second). The
   choice and the loaded issues are kept per workspace in memory until the app restarts (Refresh
   reloads); the first switch to Jira in a workspace loads once.
2. **Keys.** The workspace's Jira project keys are `project.jira_keys` (§5, §7). Without keys,
   the Jira side shows "Link this workspace to Jira projects", a field (`PROJ, BILL`, focused)
   and Save; Enter also saves, Esc clears the text. A daemon error shows under the field as
   `✕ <message>` (e.g. a key that is not letters, digits and `_`). Save sends `project.update`
   `jira_keys` (the daemon trims and upper-cases, so `proj` becomes `PROJ`) and then loads the
   issues. With keys the header reads `PROJ, BILL · Edit · All | Mine · List | Kanban · Refresh`: Edit shows the
   same field pre-filled (Esc on an empty field leaves it), `All | Mine` picks every open issue
   or only the user's and searches again; the choice is `[jira] mine` (default `false` = `All`),
   one for every workspace, saved at once and kept across restarts (other workspaces' lists are
   dropped and load again on Refresh or the next switch to Jira). `List | Kanban` picks the
   layout below; the choice is `[jira] layout` (default `list`), one for every workspace, saved
   at once, and keeps the loaded issues (ADR 0077). Refresh searches again.
3. **List.** One search, `POST /rest/api/3/search/jql`, fields `summary,status,issuetype,parent`,
   `maxResults` 50 per page; while an answer has `nextPageToken` and not `isLast: true`, the next
   page is asked with that token, up to 10 pages (the first 500 issues; an empty page also
   stops; no polling). The list shows once every page arrived. JQL `project in (PROJ, BILL) AND
   statusCategory != Done ORDER BY updated DESC`; `Mine` adds `AND assignee = currentUser()`
   before `statusCategory`. While it runs:
   "Loading Jira issues…" with loading dots. A row: issue type, key, summary (one line, cut with
   `…`), status and, when a task is linked, that task's column (`Todo`, `Running`, `Review`,
   `Done`) as a chip. Nothing found: "No open issues in PROJ, BILL" (`Mine`: "No open issues
   assigned to you in PROJ, BILL"). The "Filter issues" field keeps the issues whose key or
   summary contains its text (ignoring case); none left: `No issues match "<text>"`.
   - **List layout:** a tree, depth first: each issue under its parent (epic > task >
     sub-task), indented one step per level. Roots come in the order their first issue appears
     in the search, children in search order. A parent outside the results (done, or not the
     user's) shows as a header row from the `parent` field: type, key and summary in muted text,
     no status; a click opens it like any row. A row with children has a chevron that folds
     them (kept per workspace in memory until the app restarts). With a filter, a row shows when
     it or a row under it matches, and folded rows open.
   - **Kanban layout:** one column per status: `In Progress` first, `To Do` second, other
     statuses after in first-seen order; each column heads with its status icon, name and
     count and scrolls on its own; the board scrolls sideways below the column width. A card:
     summary (two lines at most), type icon, key, area chip, the linked task's chip and, when
     the issue has a parent, the parent's key and summary. Read only: cards do not drag.
4. **Open.** A task belongs to issue `PROJ-123` when it is in the workspace and its title starts
   with `PROJ-123 ` (the key and one space; `PROJ-12` does not match `PROJ-123 x`; no column,
   ADR 0066). Click a row with a linked task: the Board switches to `Tasks` and the task's edit
   dialog opens; no second task is made. Click a row without one: the row shows loading dots
   while `GET /rest/api/3/issue/PROJ-123?fields=summary,description` runs, then the New task
   dialog opens with title `PROJ-123 <summary>`, the workspace's default agent, "Run in a
   worktree" off (the dialog's own choice), and the prompt
   `PROJ-123: <summary>\nhttps://<site>/browse/PROJ-123`, then `\n\n<description>` when there is
   one. Save creates a Todo task as usual; the row then shows `Todo`. A reply that arrives
   after another dialog opened, or after the workspace changed, is dropped (click the row
   again); a click on another row meanwhile replaces the first.
5. **Text.** The description arrives as Atlassian Document Format and becomes plain text:
   `text` → its text; `hardBreak` → a line break; `paragraph` and `heading` → the text followed
   by a blank line; `bulletList` / `orderedList` items → `- ` / `1. ` lines (nested lists
   indented); `codeBlock` → a ``` fenced block; `mention` → its `attrs.text`; `inlineCard` and
   `blockCard` → `attrs.url`; any other node → its children; a node without `content` → nothing;
   a `null` description → nothing.
6. **Errors.** The Jira side shows `Cannot load Jira issues.`, the reason and a Retry button;
   a failed issue fetch shows `✕ <reason>` under its row. Nothing is retried automatically.

   | Cause | Message |
   |---|---|
   | `email` empty | `Set [jira] email in settings.toml` |
   | Token variable unset or blank | `Set the environment variable <NAME> to your Jira API token` |
   | 401 / 403 | `Jira rejected the login: check [jira] email and the API token` |
   | 400 | Jira's `errorMessages`, joined (e.g. an unknown project key) |
   | 404 on an issue | `PROJ-123 no longer exists or you cannot see it` |
   | Other status | `Jira answered <status>: <first 200 chars of the answer>` |
   | Network / timeout (20 s) | `Cannot reach https://<site>: <error>` |
7. **Token.** Basic auth `email:token`; the token is read, on every call, from the environment
   variable `[jira] api_token_env` names (default `JIRA_API_TOKEN`), never from `settings.toml`,
   and never appears in a message or a log. `site` may be a bare host or a URL; every request
   goes over `https://`, whatever scheme `site` had, except to `127.0.0.1` / `localhost`
   (tests).

### §20J Tray, taskbar badge and mark unread (outside the milestones, ADR 0064)

Notice who needs you with the window minimized, hidden or behind others. "Needs you" is the
sidebar chip's count (§10, ADR 0036): needs input, plus finished and not yet viewed.

1. **Taskbar badge (Windows).** Every Space window's taskbar button shows an overlay icon with that
   count (`ITaskbarList3::SetOverlayIcon`; amber, `9+` from 10; description "3 need you");
   cleared at 0. macOS: the Dock icon's badge shows the same count (`99+` from 100, §20L). Linux: nothing.
2. **Flash (Windows).** When a session enters `needs_input` while the window is not focused, the
   taskbar button flashes until the window is focused (`FlashWindowEx`); it stops on focus and when
   nothing needs you any more. `[notify] flash_taskbar = false` turns it off (applies live). macOS:
   the Dock icon bounces once (§20L); Linux: nothing.
3. **Tray icon (Windows).** Owned by the UI process, shown while it runs. Icon: Mkrew's own while
   nothing works or waits, a dot in the working color while an agent works, a dot in the needs-input
   color while something needs you; tooltip `Mkrew · 2 need you · 1 working`. Right-click menu: up to
   8 sessions that need you (asking for input first, then newest first), `agent · workspace — state, age` (a click shows the window
   and opens that pane); a greyed `● N working`; `Open Mkrew`; `Quit UI (agents keep running)`. A
   left click opens the window.
4. **Close to tray (Windows; macOS in §20L).** `[ui] close_to_tray = true` (default): ✕ and Alt+F4 hide the window
   instead of closing it (no pane is attached while hidden, §10). The tray menu, an OS notification click and
   starting Mkrew again (same exe, same daemon socket) show it. `false`, or no tray icon: ✕ quits the UI as before
   (unsaved Editor changes ask first). `Quit UI` shows the window if unsaved Editor changes need an
   answer, then ends the UI; the daemon and every agent keep running (§2.2).
5. **Mark unread.** Pane menu (right-click a sidebar session row, or Shift+F10 with a terminal focused; a pane has no header to right-click) **Mark
   unread** and palette "Agent: Mark unread" (no key) put a finished agent session (done, or idle
   after work) back into Needs you — chip, row tint, Inbox, badge, tray — until its pane is focused
   again in the active window or its state changes. Greyed for a session that already needs you,
   works, or is a terminal. Saved with "viewed" (below).
6. **Cost.** Counts come from the sessions the UI already holds; the badge, the icon and the menu
   are updated only when the count (or the listed set) changes, never per frame.
7. **Viewed is kept.** Which finishes were seen, which idle agents went quiet after work and which
   sessions were marked unread survive a UI restart in `<data_dir>/ui_viewed.json`, written only
   when it changes and pruned to the listed sessions. A missing or corrupt file is an empty state;
   the daemon never reads it.

### §20K In-app update (outside the milestones, ADR 0071)

A tag push publishes a release; a running Mkrew offers it and installs it in place.

1. **Release.** `v*` tag whose number equals the root `Cargo.toml` `version` (the workflow
   refuses others). The Release workflow publishes (no draft) `mkrew-<v>-x64-setup.exe`,
   `mkrew-<v>-macos-arm64.zip` (`Mkrew.app`, Developer ID signed and notarized when the repository
   has the Apple secrets, else ad-hoc signed; §20L) and the Linux tar.gz.
2. **Check.** The UI asks `https://api.github.com/repos/tungmk97/mkrew/releases/latest` at window
   open and every 6 h (10 s timeout, background executor), and on palette "Global: Check for
   updates" (no key). Only a plain `a.b.c` tag strictly greater than the running version counts.
   404, network or parse errors show nothing (one log line); the palette check answers with a
   toast ("… is up to date", "… is available", or the error). A newer version shows the update
   button (§14) and one info toast "Mkrew <v> is available".
3. **Install kind.** Windows: `<exe dir>/../unins000.exe` exists → Inno setup, per-user when the
   exe is under `%LOCALAPPDATA%`. macOS: the exe is `<X>.app/Contents/MacOS/mkrew` → app bundle;
   a Gatekeeper-translocated copy (read-only, under `AppTranslocation/`) maps to the real bundle
   it mounts (`statfs` `f_mntfromname`), so a quarantined app opened from Downloads updates too.
   Anything else (dev build, MSI install, Linux, Intel Mac, no matching asset): the button opens
   the release page.
4. **Apply.** Unsaved Editor changes are asked about first. A confirmation (`window.prompt`):
   "Update to Mkrew <v>?", detail "Mkrew closes, installs and reopens. Agent panes resume their
   conversation." + "N agents are working; their current turn stops." when any is + "Shells and
   run commands without autostart do not come back."; Update / What's new (opens the release page,
   then asks again) / Cancel. The asset is downloaded to
   `<temp>/mkrew-update/` (its name must be a plain file name, its length the release's `size`).
   Windows: the setup runs detached with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /LAUNCH=1` +
   `/CURRENTUSER` or `/ALLUSERS` (UAC only then), the UI quits; setup stops the daemon, installs
   and reopens Mkrew as the logged-in user (`runasoriginaluser`). macOS: `ditto` unpacks beside
   the bundle, the bundle's `mk daemon stop`, the bundle is renamed `.old` (put back on failure),
   the new one moved in, its `mkrew` started directly (keeps the environment, so `PATH`), the UI
   quits; `.old` and the unpack folder are removed at the next start. Editor edits made while it
   downloaded are asked about before the installer runs. A failure
   before the quit shows an error toast and keeps the app running.
5. **What survives.** The new UI starts a new daemon, which resumes agent panes and tasks (§8).
   No checksum or signature beyond HTTPS.

### §20L macOS app integration (outside the milestones, ADR 0100)

What a Mac user expects of an app around its windows and the Dock.
macOS only unless a line says "every OS".

1. **Menu bar.**
   `Mkrew`: About Mkrew (the standard panel, from the bundle's Info.plist), Settings…, Check for Updates… (not in a machine window), Services, Hide Mkrew, Hide Others, Show All, Quit Mkrew.
   `Edit`: Copy, Paste.
   `View`: Enter Full Screen.
   `Window`: Minimize, Zoom.
   Settings, Copy, Paste and Enter Full Screen are the keymap's own commands (`open_settings`, `copy`, `paste`, `toggle_full_screen`), so the menu shows the user's keys.
   Quit (Cmd+Q), Hide (Cmd+H), Hide Others (Option+Cmd+H) and Minimize (Cmd+M) are menu-only keys; a key bound in `keymap.toml` wins.
   Copy and Paste act in terminals only: the Files editor and text fields take key events, not menu actions (their keys work).
2. **Quit.**
   Cmd+Q and Quit Mkrew run "Quit UI" (§20J): unsaved Files changes and prompt drafts are asked first, the machine windows are asked to quit, the daemon and the agents keep running.
   Dock → Quit and log-out end Mkrew without a question (gpui cannot veto them): the window places are saved, the machine windows are asked to quit, unsaved Files changes are lost.
3. **Close.**
   While `[ui] close_to_tray` is on (default), the red button hides the app like Cmd+H; a click on the Dock icon, or Cmd+Tab, shows it again with its state (panes, unsaved Files changes, detached windows).
   The Dock badge and bounce keep working while hidden.
   `false`, or a machine window: the red button asks about unsaved changes and quits.
4. **Window place (every OS).**
   Each Space window's restore rect, maximized and full screen state are kept in `<data>/windows.json` (`spaces`, ADR 0115; a machine window: `windows-<key>.json`) and restored at the next start; a place on a monitor that is gone moves to the primary display; the size is never below 1100 x 560; a missing or broken file opens one window centered at 1200 x 760 as before.
   A detached window keeps full screen too (`fullscreen` in `windows.json`; absent = false).
   New and restored windows are kept inside a display's visible area (menu bar and Dock excluded).
5. **Traffic lights.**
   Centered on the title bar at any `[ui] font_size` (y = (bar height - 14) / 2, set when the size changes); no left gap for them in full screen.
6. **Dock.**
   The badge shows the Needs-you count (§20J), `99+` from 100, cleared at 0.
   When a session enters `needs_input` while no Mkrew window is active and `[notify] flash_taskbar` is on, the icon bounces once.
7. **PATH.**
   The first program lookup asks the login shell (`$SHELL -l -i -c`, `/bin/zsh` if unset, 5 s limit) for its `PATH` and puts it before the process's own; the process environment is not changed.
   It is handed explicitly to: agents and shells and the AI helper commands the daemon starts (`launch::command`), every `git` the daemon runs (found on it first, so a Homebrew git wins; its hooks, gpg, git-lfs and credential helpers get it), the `ssh` of a machine connection (a `ProxyCommand` naming a Homebrew tool works), and `zed`; `lookup::find` uses it for `zed`, `tailscale` and agent commands.
   Other programs the UI starts by name (`open`, `ssh` itself) live in `/usr/bin`; the daemon and `mk` are started by full path, and the daemon warms its own login `PATH` at start.
   The default shell is started as a login shell.
   If the shell fails or times out, the process's own `PATH` is used.
8. **`mk` command.**
   Palette "Global: Install mk command" (listed only in `Mkrew.app`) links `/usr/local/bin/mk`, else `~/.local/bin/mk`, to the bundle's `mk`; a toast names the link and says when its folder is not on `PATH`.
   When `/usr/local/bin` holds an older link of ours that cannot be replaced (root-owned), the install falls back to `~/.local/bin` and the toast says the older link shadows the new one and gives `sudo ln -sf '<mk>' /usr/local/bin/mk` to fix it.
   A translocated copy (§20K) links the real bundle's `mk`; it is refused only when that bundle could not be found (the path still holds `/AppTranslocation/`), as the link would dangle.
   A link already pointing at that `mk` is left as it is; a link to anything but a bundle's `Contents/MacOS/mk`, or a real file named `mk`, is never replaced.
   The error hint is `sudo mkdir -p /usr/local/bin && sudo ln -s '<mk>' /usr/local/bin/mk`.
9. **Packaging.**
   `Mkrew.app` has an icon (`Mkrew.icns`) and leaves the Windows-only profiles out of its `agents/` copy.
   The release job signs with Developer ID (hardened runtime, microphone entitlement), notarizes and staples when the repository has the secrets `MACOS_CERTIFICATE_P12`, `MACOS_CERTIFICATE_PASSWORD`, `MACOS_SIGN_IDENTITY`, `APPLE_ID`, `APPLE_APP_PASSWORD` and `APPLE_TEAM_ID`; otherwise it signs ad-hoc, and Gatekeeper blocks the first open (README: Open Anyway on macOS 15 and later, or `xattr -dr com.apple.quarantine`).
10. **Not done.**
    Single instance (LaunchServices reuses a running bundle; a terminal launch shares the daemon), a Dock menu, raising a running machine window from the launcher, and a veto of Dock Quit / log-out.
    To verify on a Mac: AppKit lists the open windows in the Window menu by itself (gpui registers our Window menu as the system windows menu), and View shows exactly one Enter Full Screen; if AppKit adds its own next to ours, ours is dropped on macOS.
    Each `mkrew --machine` window is its own process with its own Dock icon (ADR 0089).

### §20M Browser pane (outside the milestones, ADR 0111)

A web page as a fourth kind of pane, next to sessions, files and the diff (§5, §10).
Windows and macOS show the page with the OS web view; Linux and machines without it use the system browser.

1. **Opening.**
   A web link (`http` or `https`) in a Chat message, a Markdown preview, a Docs page, a terminal (Ctrl+Click, Cmd+Click on macOS) or link hints opens in a browser pane; `mailto` and other schemes go to the OS as before.
   If the tab shown in the window has a browser pane, the link loads there (the first one in tree order) and focuses it; a zoomed neighbour is unzoomed first.
   Otherwise the link splits right of the pane it came from (the tab's focused pane when it is not known).
   A link in a view with no tab (Docs) opens a new tab and brings Terminals to the front.
   Hovering `+` shows a Browser item (Agent, Shell, Note, Browser, `▾`): a click opens a new tab with an empty browser and its address field focused, a drag splits a pane at the drop edge; the `▾` menu has a "Browser" row.
   `Alt+Shift+B` (`⌥⌘B`) splits right with an empty browser; its palette label is "Panes: New browser pane".
2. **Look.**
   A browser pane has the frame of every pane (stripe, dim, grip, divider, drop overlay); the dim covers the toolbar only, because GPUI cannot paint above the native page.
   Its tab chip shows the globe and the page title, else the URL, else "New browser".
   The toolbar is one row: back, forward, reload (stop while loading), the address field, Open in system browser, close.
   Back and forward are enabled whenever the pane has a page and disabled when it has none; the platform's history is not queried.
   A 2 px accent line under the toolbar shows while the page loads, without animation.
   An empty pane shows "Type an address or paste a link" below the toolbar.
   Load errors (no server, no DNS) are the platform's own error page.
3. **Address field.**
   Enter loads the typed text, Esc restores the URL and gives the keyboard back to the page, `Ctrl+L` (`⌘L`) focuses it and selects all.
   Empty: nothing loads.
   A scheme (`https:`, `http:`, `file:`, `about:`): kept as typed.
   `localhost`, an IPv4 or bracketed IPv6 address, with or without a port and a path: `http://` in front.
   No space and a dot in the host part: `https://` in front.
   Anything else: a Google search of the text.
4. **Pages.**
   The leaf's `web` field follows the page and is saved when a load finishes, so a restart brings the pane back on its last page; back / forward history is not kept across a restart, cookies and logins are.
   A page that asks for a new window loads in the same pane.
   Windows keeps its data in `<data>/mkrew/WebView2` (shared by all panes, apart from the system browser); macOS uses the app's default data store.
   F12 opens the platform inspector; on Windows the page follows Mkrew's theme (`prefers-color-scheme`), on macOS it follows the app appearance.
   Downloads, permission prompts and file pickers are the platform's.
5. **Native view.**
   The view is a native child of the window (`wry` 0.57) below the toolbar; it paints above GPUI, so Mkrew hides it with every overlay: palette, dialog, menu, the `+` menu, a peek popover and any drag (the title card, the page's title and URL, shows in its place), with a tab or view change, a zoomed neighbour and while it is off screen.
   Moving a browser pane to another window (tear-off, Move back, a drag, a closing detached window) rebuilds the page there on the same URL, so back / forward history is lost; `wry` 0.57 `reparent` keeps the first parent's keyboard target, and macOS would need `unsafe`.
   On Windows `GPUI_DISABLE_DIRECT_COMPOSITION=1` is set at the start of `main()` unless the user set it or the WebView2 Runtime is missing, because gpui-pre-windows 0.3.7 draws through a topmost DirectComposition visual that covers child windows; it is removed from the environment of the daemon, Zed, the updater and machine windows.
   The first build of a page pumps messages inside GPUI's borrow, so a click or key in the first ~400 ms may be dropped (accepted).
6. **Keys.**
   A click in the page gives it the keyboard and focuses its pane in Mkrew (stripe, tab chip and the palette's pane commands follow).
   An initialization script gives back every combo with Ctrl, Alt or Cmd (and F-keys) of a global binding or of the browser context (`Ctrl+L`); `Ctrl+` / `Cmd+` A, C, V, X, Y and Z, every key without a modifier and a lone Alt stay in the page.
   So `Alt+←` / `Alt+→` move pane focus, not page history; the page's history is the toolbar, the mouse buttons and the palette.
   F5, Ctrl+R, F12 and zoom keys work in the page; on macOS Cmd+R and Cmd+0 are Mkrew's own bindings, so the page gives them back.
   When focus moves to another pane or an overlay opens, the keyboard goes back to Mkrew.
   The key list follows the keymap (ADR 0073).
   A page is not trusted: only a page on screen in the active window can claim focus or post keys, and a key reaches only global and browser-context commands (a page can fake those, never a Git, Board, editor or terminal command).
7. **Where it is not available.**
   Linux and Windows without the WebView2 Runtime use the system browser: links call the OS, with one toast per run ("Web pages open in your browser: <reason>").
   The `+` row has no Browser item, `Alt+Shift+B` does nothing and the palette lists no "Panes: New browser pane", "Browser: Focus address", "Back", "Forward" or "Reload" (only "Open in system browser" stays on a browser pane).
   A browser pane already in a layout (written on another machine, or restored) stays and shows the reason and an "Open in system browser" button.
   If building a view fails on a machine that has the Runtime, the pane shows the error and the button; the next link or Enter in the address field tries again.
   A `localhost` page is the UI machine's localhost, not a remote daemon's.
8. **Not done.**
   Picture in picture cards, agents reading or driving the page, an element picker, screenshots into the composer, a browser on Linux, port forwarding of a remote machine, back / forward history across a restart, a setting that sends links to the system browser, find in page, bookmarks, a tab strip inside the pane and a download panel.
