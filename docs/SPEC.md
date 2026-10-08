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
- §12A UI: Files view
- §12B UI: Git view
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
- Browse project files and view them read-only; see git changes, stage, commit, and compare
  against a base branch (default `main`, configurable per workspace) like Orca; comment on diff
  lines and send the comments to an agent.
- Simple, calm, beautiful UI — soft, not rigid.
- v2 (§20, M9–M11, M13): make the daily loop with agents faster — notice who needs you, answer
  without switching, instruct precisely, review and undo per turn, and isolate parallel work in
  worktrees when asked.

**Non-goals**

- Not a code editor: no editor, no LSP. Files and diffs are read-only; editing happens in Zed.
- No mobile app, cloud sync or user accounts.
- No GitHub/Linear integration in v1.
- Mkrew never calls LLM APIs itself; it only runs CLI agents the user installed.

**Naming**

| Thing | Name |
| --- | --- |
| App (UI binary) | `mkrew` |
| Daemon binary | `mkrew-daemon` |
| CLI binary | `mk` |
| Crates | `mkrew-core`, `mkrew-protocol`, `mkrew-store`, `mkrew-daemon`, `mkrew-cli`, `mkrew-ui`, `mkrew-fakeagent` |
| Env vars | `MKREW_SESSION_ID`, `MKREW_SOCKET`, `MKREW_LOG` |
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

**Lifecycle**

- UI starts → connects to the daemon; if none is running it spawns `mkrew-daemon` detached,
  then reconnects.
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
| OS notifications | `notify-rust` | |
| Paths | `directories`, `dunce` | |
| Markdown | `pulldown-cmark` | docs review (M6) |
| Syntax highlighting | `syntect` (pure-Rust regex feature, lazy-loaded) | files & diffs (M7) |
| Diagrams | `mermaid-rs-renderer` (no default features) | Mermaid in the Docs preview (ADR 0021) |
| File walking | `ignore` | respects .gitignore (M7) |
| Fuzzy matching | `nucleo-matcher` | quick open files (M7) |
| Diff | `similar` | |
| Errors | `thiserror` (libs), `anyhow` (bins) | |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` | |
| Tests | `insta`, `tempfile` | |

**Repo layout**

```
mkrew/
├── Cargo.toml                # workspace
├── AGENTS.md / CLAUDE.md     # agent instructions
├── docs/                     # SPEC, PROGRESS, BUILD_LOOP, HUMAN_CHECKS, decisions/, screenshots/
├── scripts/                  # verify, autobuild, status, package (MSI / setup .exe / tar.gz)
├── packaging/                # mkrew.wxs (WiX 7 MSI), mkrew.iss (Inno Setup 6 setup .exe)
├── agents/                   # sample profiles: claude.toml codex.toml omp.toml shell.toml fake.toml
└── crates/
    ├── mkrew-core/           # models, config, profile parsing, state machine. NO I/O beyond files.
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
  title             TEXT,
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
  kind       TEXT NOT NULL,              -- state_changed|hook|files_changed|...
  payload    TEXT NOT NULL,              -- JSON
  at         INTEGER NOT NULL
);

-- migrations/0002_terminals.sql (M2)
CREATE TABLE terminal_tab (
  id         TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id),
  title      TEXT NOT NULL,
  position   REAL NOT NULL,
  layout     TEXT NOT NULL               -- JSON split tree, leaves are session ids
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
  env_file   TEXT,                       -- relative to dir, e.g. ".env"
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
`project.color TEXT` (ADR 0020).

Split-tree example (`terminal_tab.layout`): vertical split, right half split horizontally.
`vertical` = children side by side (first left), `horizontal` = stacked (first on top); `ratio`
is the first child's share; every split has exactly two children; `null` = a tab without panes.

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

[state]
mode = "hook+screen"            # "hook" | "screen" | "hook+screen"
idle_after_ms = 3000
needs_input_patterns = ['(?i)do you want to', '(?i)\(y/n\)', '❯ 1\. Yes']
```

```toml
# agents/codex.toml
name = "Codex"
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
command = "omp"
prompt_mode = "type"
resume = "omp --resume {session_id}"   # {session_id} reported by the Mkrew extension
oneshot = "omp -p --no-session"  # one-shot text; prompt on stdin
[state]
mode = "hook+screen"
idle_after_ms = 3000
needs_input_patterns = []
```

```toml
# agents/copilot.toml
name = "GitHub Copilot"
command = "copilot"
prompt_mode = "type"             # copilot takes no positional prompt
resume = "copilot --resume={session_id}"
# no oneshot: `copilot -p` takes the prompt as an argument, not on stdin
[state]
mode = "hook+screen"
idle_after_ms = 3000
needs_input_patterns = []
```

`agents/shell.toml` (kind shell, command = OS default shell) and `agents/fake.toml`
(command = `mkrew-fakeagent`, see §16) also ship. Optional key `kind = "agent" | "shell"`
(default `agent`); a shell profile may omit `command` to use the OS default shell
(`%COMSPEC%` / `$SHELL`). Unknown keys and invalid `needs_input_patterns` regexes are parse errors.
Profiles written before `oneshot` existed are not updated (seeding never overwrites); ✦ then
answers `invalid_state` naming the key and the file.

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

**Hook integrations — `mk integration install|uninstall|status <agent>`**

- The daemon sets `MKREW_SESSION_ID` and `MKREW_SOCKET` in every PTY it creates.
- Claude Code: add hooks to `~/.claude/settings.json` for `SessionStart`, `UserPromptSubmit`,
  `Notification`, `Stop`, each running `mk hook claude <event>`. The hook reads JSON from stdin and
  takes `session_id` as `native_session_id`.
- Codex: set `notify = ["mk", "hook", "codex"]` in `~/.codex/config.toml`; Codex passes JSON as an argument.
- Oh My Pi: `mk integration install omp` writes the Mkrew-owned extension
  `<PI_CODING_AGENT_DIR or ~/.omp/agent>/extensions/mkrew.ts`; it runs
  `mk hook omp <busy|needs_input|done|session_start> {"session_id": …}` (root session only: not
  with `OMPCODE=1`, not without a UI).
- GitHub Copilot CLI: `mk integration install copilot` writes the Mkrew-owned
  `<COPILOT_HOME or ~/.copilot>/hooks/mkrew.json` (`sessionStart`, `userPromptSubmitted`,
  `agentStop`, `notification` matching `permission_prompt|elicitation_dialog`), each
  `exec: mk hook copilot <event>`, JSON on stdin, `sessionId` = `native_session_id`.
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
| `project.list/add/update/archive` | `path`, `name`, `run_commands`, `run_autostart`, `color` (update only), `review_globs`, `default_agent?` | Project (`update`: absent field = unchanged, `null` = cleared; `run_commands` replaces the list; each command is trimmed and checked (name 1–20 chars without `/` `:`, unique; one-line command; relative `dir`/`env_file` inside the project; env keys `[A-Za-z_][A-Za-z0-9_]*`; max 8); `default_agent` must be an agent profile; `color` must be one of red, orange, yellow, green, teal, blue, purple, pink (`null` = none)); `archive`: `project_id` → Project (`archived: true`, also `project.updated`): its live sessions are asked to stop, clients leave archived projects out, `add` of an archived project's folder un-archives it (same id, tasks and history) — ADR 0019 |
| `project.run_start` | `project_id`, `name?`, `cols`/`rows` (default 120×30) | `RunStarted { tab_id?, sessions: [{name, session_id, started}] }` — a run command (ADR 0018) runs as the project's live `shell`-kind session titled `run:<name>`: without `name` every command not already running starts; a running one is reported with `started: false`. cwd = `<project>/<dir>`; env = the shell profile's, then the env file, then `env`; the command is typed into the shell followed by an exit for that shell family. Every command is validated before anything spawns (missing folder, unreadable env file → `invalid_request` naming the command). The live run sessions lie side by side in the project's `run` tab (created when missing; layout rebuilt on every start, config order). No commands: `invalid_state`; unknown `name`: `not_found` |
| `project.run_restart` | `project_id`, `name?`, `cols`/`rows` | `RunStarted` — stops the targets like `run_stop` (5 s grace, then forced kill), waits until they exited (max 10 s, else `timeout`, nothing started), then starts them like `run_start` |
| `project.run_stop` | `project_id`, `name?` | ok — asks the run sessions to terminate (without `name`: every live run session of the project, also one whose name left the config); ok when nothing runs |
| `session.ports` | `project_id` | `[{session_id, ports}]` of the project's live sessions with ports (kept in daemon memory only) |
| `task.list/create/update/move/delete` | `project_id`, `status`, `position` | Task (`list`: Task[], filters `project_id?`/`status?`; `create` = Todo card, `title`, `prompt`, `agent_profile?` (none = project `default_agent` at run time, stored as `''`), `position?` (after the last Todo card); `update`: `title`/`prompt`/`agent_profile`, absent = unchanged, `null` agent = default; `move`: `status` + `position`; `delete` returns ok, `invalid_state` while a session of the task is not exited, its exited sessions stay as history with `task_id` = NULL — ADR 0004) |
| `task.run` | `task_id`, `cols`, `rows` | `session_id` |
| `task.undo_preview` | `task_id` | `checkpoint`, `fingerprint` (opaque), `changed[]` (tracked files restored), `added[]` (tracked since the checkpoint, deleted), `created[]` (untracked files created since, deleted), `diff` (≤ 1 MiB) + `diff_truncated`; `invalid_state` without a checkpoint (ADR 0005) |
| `task.undo` | `task_id`, `confirm: true`, `created[]` + `fingerprint` (from the preview) | ok — restores the project folder to the checkpoint and deletes the previewed `created` files that are still new; `invalid_request` without `confirm`, `invalid_state` while a session of the task is live, without a checkpoint, or when the tracked changes no longer match `fingerprint` |
| `session.spawn` | `project_id`, `agent_profile?`, `kind`, `tab_id?`, `title?`, `cols`, `rows` | `session_id` — no `agent_profile` = project `default_agent` (agent) or `shell` (shell); a named profile sets the kind; a tab with `null` layout gets the session as its only pane |
| `session.list` | `project_id?` | Session[] |
| `session.attach` | `session_id` | screen snapshot (grid + cursor + modes) + subscribe to output |
| `session.detach` | `session_id` | ok |
| `session.input` | `session_id`, `data` (base64) | ok |
| `session.send` | `session_id`, `text`, `submit` (bool) | `send_id`, `queued` (from M10, §20B.2) — queued if the session is working; multi-line text goes as a bracketed paste when the program enabled mode 2004 |
| `session.wait` | `session_id`, `until` (states), `timeout_ms` | final state |
| `session.resize` | `session_id`, `cols`, `rows` | ok |
| `session.kill` | `session_id`, `force` | ok |
| `session.remove` | `session_id` | ok — leaves the session out of `session.list` for good (row, log, turns kept); a live one is asked to stop first (ADR 0019) |
| `session.history` | `session_id` | `files[]`: existing session log files, oldest first (`<id>.log.1`, `<id>.log`); works for exited sessions (also from before a restart) |
| `tab.list/create/rename/close/move` | `project_id`, `title`, `position` | TerminalTab (new tabs: `layout` = `null`; close deletes the tab, its sessions keep running, no event) |
| `tab.set_layout` | `tab_id`, `layout` | ok |
| `hook.report` | `mkrew_session_id`, `agent` (`claude`/`codex`/`fake`), `event`, `payload` | ok — `not_found`/`invalid_state` unless the session is live; stored as a `hook` session event |
| `agent.profiles` | – | AgentProfile[] + parse errors |
| `doc.list` | `project_id` | `DocFile[]` sorted by path: md files matching review globs + md files in `files_changed` session events of the last 24 h + files with unresolved comments (existing files only); each with `open_comments` (unresolved thread roots), `last_session_id?`/`last_changed_at?` (newest such event). Keeps the project watched for `doc.changed` until the daemon stops |
| `doc.read` | `project_id`, `file_path` | `file_path`, `content`, `hash` (opaque); ≤ 2 MiB UTF-8; `invalid_request` for absolute/`..` paths. Also keeps the project watched |
| `comment.list/create/reply/resolve/reopen` | `file_path`, `start_line`, `end_line`, `quote`, `body` | DocComment (every comment carries `context`: `doc` (default) or `diff`, replies = their root's; `list`: `project_id` + `file_path?` + `context?` → DocComment[], roots and replies oldest first; `create`: `project_id`, 1-based inclusive lines, `context?` (`diff` = new-side lines of a diff); `reply`: `comment_id`, `body`, `mkrew_session_id?` (set by `mk comment reply` = agent author); `resolve`/`reopen`: `comment_id`). Ids are `c_` + ULID; the prefix is optional when parsing |
| `comment.send_to_agent` | `comment_ids[]`, `session_id` | the thread roots, now `sent` (`sent_to`, `sent_snapshot` = file content), request order; reply ids stand for their thread; all on one file of the live session's project and of one `context` (`diff` → the prompt asks to "Fix the code in <file>" instead of "Update <file>", without "Only edit this file"). The §12 prompt is queued like `session.send` (submit); multi-line sends are wrapped as a bracketed paste when the program enabled mode 2004 |
| `fs.list` | `project_id`, `dir`, `show_ignored` | children (lazy) with git status |
| `fs.read` | `project_id`, `path` | content (≤ 2 MB), binary flag, language |
| `fs.find` | `project_id`, `query`, `limit` | fuzzy file matches |
| `git.status` | `project_id`, `mode` (`working`/`base`), `since?` (base mode: compare from this commit instead of the merge-base, e.g. `task.checkpoint`; `merge_base` = it) | `branch?`, `head?`, `upstream?`, `ahead`/`behind`, `base_ref?` + `base_auto`, `merge_base?` (base mode); changed files (`path`, `old_path?`, `kind`, `added?`/`removed?`) in `staged`/`unstaged`/`untracked` (working mode; conflicts are `unstaged`) or `against_base` (base mode) |
| `git.diff` | `project_id`, `path`, `mode`, `staged?` (working mode: index side), `commit?`, `since?` (base mode, as in `git.status`) | `binary`, `too_large`, `added`/`removed`, `hunks[]` (`old/new_start/lines`, `header`, `lines[]` with `kind`, `text`, `old_line?`/`new_line?`, `no_newline`, `crlf`) |
| `git.stage` / `git.unstage` | `paths[]` or `hunk` | ok |
| `git.discard` | `paths[]`, `confirm: true` | ok (destructive; UI always confirms first) |
| `git.commit` | `message` | new commit hash |
| `git.log` | `limit`, `path?` | Commit[] |
| `git.show` | `commit` | commit info + changed files |
| `ai.generate` | `generation_id` (client-chosen), `project_id`, `worktree_task_id?`, `agent_profile?` (none = project `default_agent`), `kind` (`commit_message`/`enhance_prompt`), `input` | `null` at once; runs the profile's `oneshot` (§6) with a 120 s limit in the project/worktree folder, prompt on stdin; answer in `ai.generated` |
| `git.set_base` | `project_id`, `base_ref` (`null`/blank = auto-detect; trimmed; refused if it starts with `-` or contains whitespace or control characters) | ok — broadcasts `project.updated` (Project carries `base_ref`) |
| `git.fetch` / `git.push` | `project_id` | ok (push = current branch to its upstream, never force) |
| `daemon.status/stop` | – | info / ok |

**Events**

| Event | When |
| --- | --- |
| `session.output` | **binary frame** (see below): raw PTY bytes, only to attached clients, batched ≤ every 16 ms |
| `session.snapshot` | only to an attached client whose output queue overflowed: fresh screen snapshot replacing the dropped output (backpressure, below) |
| `session.state` | session state changed (`state`, `source` = hook/screen/daemon, `changed_at`) |
| `session.exited` | process exited, with `exit_code` |
| `session.ports` | a run session's port list grew (`session_id`, `ports`): ports are scanned from its screen text (`http(s)://<host>:<port>`, `port(s)[:=] <n>`), first 4 distinct, never removed while it lives |
| `task.updated` / `project.updated` / `tab.updated` | entity changed |
| `task.deleted` | task removed (`task_id`, `project_id`; ADR 0004) |
| `conflict.detected` | two sessions edited the same file within 60 s (M5): `project_id`, `path`, `session_ids`, `task_ids` |
| `doc.changed` | a watched md file changed (`project_id`, `file_path`, new `hash`, `null` = deleted); its comments are re-anchored right after (`comment.updated`) |
| `comment.updated` | comment created/replied/state changed (incl. agent replies via CLI) |
| `notification.clicked` | user clicked an OS notification (UI focuses that pane) |
| `git.changed` | workspace git state changed (debounced, ≤ once per 300 ms) |
| `ai.generated` | answer to an `ai.generate` (`generation_id`, `project_id`, `text?` trimmed and unfenced, `error?`) |

**Framing.** Every frame = `u32` big-endian length + `u8` kind + payload. Kind `0x01` = JSON
(requests, responses, events). Kind `0x02` = terminal output: 16-byte binary ULID of the session
followed by raw PTY bytes — no JSON, no base64, because agents and builds can print tens of MB.
The length counts the kind byte plus the payload; frames longer than 16 MiB are rejected as a
protocol error (the connection is dropped). Small inputs (`session.input`) may stay base64-in-JSON.

**Backpressure.** Each attached client has a bounded output queue (e.g. 4 MB). If it overflows
(slow or frozen UI), the daemon drops the queued bytes and sends a fresh screen snapshot instead.
The daemon never buffers unboundedly and never blocks a PTY reader on a slow client.

**Screen snapshot.** `session.attach` returns `{state, snapshot}`; `session.snapshot` carries
the same `snapshot`. A snapshot is the visible screen of the daemon's emulator (no scrollback):
`cols`, `rows`, `lines` (one array per row of style runs `{text, fg, bg, flags}`, trailing
default blanks omitted), `cursor` `{row, col, visible}` and `modes` (`alt_screen`, `app_cursor`,
`app_keypad`, `bracketed_paste`, `mouse`, `sgr_mouse`, `focus_events`). Colors are `"default"`,
`{"indexed": n}` (0–15 = ANSI) or `{"rgb": [r, g, b]}`. `flags` bits: bold 1, dim 2, italic 4,
underline 8, inverse 16, hidden 32, strikeout 64, wide 128 — a run is all single-width or all
double-width (`wide`) chars. Output frames sent after a snapshot continue from it.

Rules: on
`protocol_version` mismatch the daemon replies `version_mismatch` and the UI asks to restart the daemon.

v2 adds requests and events in §20; each milestone that changes the protocol bumps `PROTOCOL_VERSION`.

---

## §8 Daemon

- **Startup:** take the lock (exit if another daemon runs); run migrations; open the IPC endpoint;
  mark sessions left in working/needs_input/idle as `exited`; then resume (below).
- **Spawn:** create the PTY with `portable-pty`, `cwd` = project path, env `MKREW_SESSION_ID`,
  `MKREW_SOCKET`, `TERM=xterm-256color`. The daemon removes `OMPCODE` from the PTY environment,
  so a daemon started from an omp shell does not make the omp extension turn itself off.
- **PTY reader threads:** `portable-pty` reads are blocking, so each session gets ONE dedicated OS
  thread that reads into a buffer and forwards chunks over a bounded channel to tokio. Never call
  blocking PTY reads inside async tasks.
- **Headless emulator per session:** the daemon feeds every chunk into its own
  `alacritty_terminal` instance sized to the session, with a SMALL scrollback (200 lines). It is
  used for (a) state detection on the real screen text and (b) instant attach snapshots.
  Memory budget: a cell costs tens of bytes, so large scrollback in the daemon is forbidden.
  Long history lives in the session log file on disk.
- Output fan-out per chunk: emulator → state detector, session log file (buffered writer),
  attached clients (binary frames, §7).
- **Command lookup:** on Windows try `PATHEXT` (`claude.cmd`, `.ps1` shims). Clear error if not found.
- **State detection:** one state machine per session (lives in `mkrew-core`, pure, unit-tested).
- **Send queue:** `session.send` while the session is `working` is queued and delivered when the
  session reaches idle/done/needs_input.
- **Resume:** on startup, for each task in running/needs_input whose last session has a
  `native_session_id` and whose profile has `resume`, spawn the resume command. Otherwise set the
  task to needs_input with the note "session stopped — run again?". Then, for each session marked
  `interrupted` (it exited during the daemon stop, or was still live at startup after a
  crash/reboot) that has no task, is an agent, and is still a leaf of a tab layout: spawn the
  resume command in the same pane. One that cannot resume stays exited. The mark is cleared at
  startup, so each session is resumed at most once.
- **Run commands:** after resume, `project.run_start` (all) for every non-archived project with
  `run_autostart` and at least one command (ADR 0018).
- **Notifications:** OS notification when a session enters needs_input or done; at most one per
  session per 30 s. Clicking it emits `notification.clicked`.
- **Session logs:** `<data>/mkrew/sessions/<session_id>.log`, rotated at 5 MB.
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
mk project update <project> [--base develop | --auto-base] [--run-autostart on|off]
mk project run <project> [name]      # runs the run commands in the "run" tab (prints `<name> <session id>`)
mk project restart <project> [name]  # stops, then runs them again
mk project stop <project> [name]     # stops them
mk task add <project> "<title>" [--prompt "..."] [--agent claude]
mk task list [--project P] [--status S]
mk task run <task-id>
mk task undo <task-id> [--yes]         # prints files + diff since the checkpoint; --yes discards
mk session list [--project P]
mk session history <session-id>        # prints the session's raw output log (also exited ones)
mk session spawn <project> --agent <profile>   # prints the new session id
mk attach <session-id>                 # raw-mode attach; detach with Ctrl+]
mk send <session-id> "<text>" [--no-submit]
mk wait <session-id> --until idle,needs_input,done [--timeout 60000]
mk integration install|uninstall|status <claude|codex|omp|copilot>
mk hook <agent> <event>                # called by agent hooks, not by users
mk open <project> [file[:line]]        # opens in Zed
mk comment list [--file <path>] [--project P]  # default project: MKREW_SESSION_ID's, else cwd's
mk comment reply <comment-id> "<text>" # used by agents to answer review comments
```

`mk` resolves the current session from `MKREW_SESSION_ID` when relevant (e.g. `comment reply`
records the agent as author). `mk attach` makes Mkrew usable without the UI, including over SSH.

---

## §10 UI: layout, workspaces & terminals

One window, three regions:

```
┌──────────────┬──────────────────────────────────────┬───────────┐
│ WORKSPACES   │  Todo │ Running │ Needs input │ …    │ Board     │
│ ▾ api      2 │                                      │ Terminals │
│   ● claude   │  …or the terminal tabs/panes         │ Docs      │
│   ? codex    │    (tab bar: … ▶ Run · History ·     │ Files     │
│ ▸ web     ?  │     + Agent · + Shell)               │ Git       │
│ ▸ infra      │  …or the Markdown review view        │           │
│ INBOX (2)    │                                      │           │
│ web: approve?│                                      │ ▶ ▾       │
└──────────────┴──────────────────────────────────────┴───────────┘
```

- **Sidebar:** workspaces; expanding one lists its sessions with status (§11). Below: **Inbox** —
  every session in needs_input or freshly done, newest first; click jumps to that pane.
- **Remove:** the workspace menu's "Remove workspace…" archives it after a confirmation
  (`project.archive`, ADR 0019); a session row's ✕ (on hover) removes it from the sidebar
  (`session.remove`; a running one asks first and is stopped).
- **Color:** the workspace menu's swatches give a workspace one of 8 colors, shown as a dot
  before its name (decoration only, ADR 0020).
- **Views per workspace:** Board, Terminals, Docs, Files, Git.
- **View rail** (ADR 0023): the views are a column right of the main area; the active one is
  highlighted, tooltips show their shortcuts (§13).
- **Board:** columns by task status, drag & drop. Card: title, agent, run time, session status.
  Todo cards have Run. Running cards have a **follow-up** input (sends via `session.send`).
  New task: the empty board's button, or `New task` in the Todo column header.
- **Terminals:** tab bar on top; each tab is a split tree of panes; each pane is a session.
  Sessions are `agent` (from a profile) or `shell` (pwsh on Windows, `$SHELL` elsewhere, configurable).
  New panes open in the project folder. Closing a pane with a running agent asks: kill the
  session, or just hide it (still reachable from the sidebar). Layout persists (`terminal_tab`).
  No hard limit on terminals; warn above 12 running sessions per workspace.
- **Terminal rendering:** cell grid from `alacritty_terminal`; 256-color/truecolor, cursor,
  selection & copy, paste (bracketed), scrollback (UI-side, 10,000 lines, only for attached panes),
  search, resize → `session.resize`. Performance rules (mandatory — this is the hottest path):
    - Batch adjacent cells with identical style into one shaped text run per row (as Zed's
      terminal element does). Never one GPUI element per cell.
    - Re-layout only dirty rows; cache shaped rows otherwise.
    - Present at most once per display refresh; coalesce output that arrives between frames.
    - Panes that are not visible (other tab, other workspace, minimized window) are detached:
      no parsing, no layout, no rendering. Re-attach uses the daemon snapshot.
    - Use a monospace font with a precomputed cell size; no per-cell measurement.
- **Open in Zed:** on task cards, workspace menu and Docs; runs `zed <path>[:line]`; clear error
  with install hint if `zed` is missing.
- **Run button** (ADR 0018) in the Terminals tab bar before History, and as a compact
  `[▶│▾]` / `[●│▾]` / `[✕│▾]` (full label in the tooltip, ↻ ■ in the menu) at the foot of the
  view rail in the other views: `[▶ Run │▾]` with
  no command or all stopped (▶ with no command opens Run setup, and saving there runs);
  `[● Running 2/2 · :8080 :5180 │↻│■│▾]` while running; `[✕ Running 1/2 · …]` in the warning
  color when some exited. Ports: every distinct port of the live run sessions, config order, up
  to 4 then `+N`; tooltip `name — :port …` per command. Menu ▾ (mouse-driven): one row per
  command (name, state, ports, ↻ ■ or ▶), then `Show Run tab`, `Run setup…`. Run from
  all-stopped switches to Terminals and the `run` tab; Restart and per-command Run keep the
  view. A run pane's header is `<name> · :8080`; the sidebar workspace row shows `▶` while any
  run session lives (tooltip = ports). Ports load with `session.ports` per workspace on refresh
  and stay live through the `session.ports` event. **Run setup dialog:** command cards (Name,
  Folder, Command, Env file, Env as multi-line `KEY=VALUE`), `+ Add command`, `Remove`, a
  `Start with the daemon` checkbox; Ctrl+Enter (or Enter outside Env) saves; errors show in
  the dialog. Palette: `Run: Start all`, `Run: Restart all`, `Run: Stop all`, `Run: Setup…`,
  `Run: Show Run tab`.
- **Empty states** everywhere (see §14).

---

## §11 UI: per-session status

Status shows in three places: pane header, tab label, sidebar row. Always icon + text, never
color alone.

| State | Icon | Extra |
| --- | --- | --- |
| starting | ◌ | |
| working | small spinner | elapsed time |
| needs_input | ? | highlighted pane border, listed in Inbox |
| idle | ○ | idle time |
| done | ✓ | listed in Inbox until viewed |
| exited | ✕ | exit code, "Run again" / "Resume" |
| unknown | · | |

- Pane header: icon, agent name, session/task title, time.
- Tab label: the most urgent state among its panes: needs_input > done > working > idle > exited.
- Workspace row: number of working sessions and `?` if any needs_input.
- Hovering a status shows its source (hook/screen) and when it last changed.

---

## §12 UI: Markdown review (inspired by Antigravity)

Read plans/specs written by agents, comment on passages, then send the comments back to the
agent session that wrote them so it edits the file **in the same conversation context**.

- **File list:** files matching the workspace's `review_globs` (default
  `["*.md", "docs/**/*.md", ".claude/plans/**/*.md"]`) plus `.md` files recently changed by agents.
  Show open-comment counts. A filter field at the top (`Ctrl+Shift+F`) fuzzy-filters the list by
  path; ↑ / ↓ in it show the previous / next file.
- **Rendering:** `pulldown-cmark` → GPUI elements: headings, paragraphs, lists, task lists, tables,
  code blocks, links, quotes. Toggle to source view with line numbers. Code blocks are
  highlighted by their fence language (syntect, as in the Files viewer); the text uses the doc
  typography of §14 (ADR 0021). ```` ```mermaid ```` blocks are drawn as diagrams (pure Rust
  renderer, theme colors); a block that cannot be drawn shows its source and the reason.
- **Comments:** pick lines (click a preview block or a source line, Shift+click extends) →
  "Add comment". Anchored by line range + quoted text (the picked source lines). Threads with
  replies, resolve, reopen. Listed in the right margin in anchor order; clicking a thread's
  lines highlights them (ADR 0006; character selection and anchor-aligned cards are backlog).
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

## §12A UI: Files view

- **Tree** (left): lazily loaded per folder, respects `.gitignore` (toggle to show ignored),
  git status markers next to names (M modified, A added, D deleted, ? untracked), a small dot on
  folders containing changes. Updates live when agents create/delete/rename files.
- **Viewer** (right): read-only, line numbers, syntax highlighting via `syntect` (lazy-loaded
  syntax set). Files > 2 MB or binary: show a notice and "Open in Zed".
- `.md` files offer "Review in Docs".
- **Quick open** (`Ctrl+Shift+O`, or the "Search files…" entry at the top of the tree)
  fuzzy-finds files of the current workspace, in the folder the Files view shows (project or
  task worktree) (`ignore` to walk, `nucleo-matcher` to rank).
- Select a line → "Open in Zed" at that line.
- **Find in file** (`Ctrl+Shift+F` or "Find" in the viewer header): case-insensitive text search
  in the opened file; matches highlighted, Enter / Shift+Enter move between them (the line is
  selected for "Open in Zed"), Esc closes.

## §12B UI: Git view (inspired by Orca)

All git actions are user-initiated; Mkrew never commits or switches branches by itself.
The daemon shells out to `git` (porcelain v2, `-z` output) behind a `GitBackend` trait, so a
`gix`-based backend can replace it later without touching callers. Process spawning is slow on
Windows (tens of ms, worse with antivirus), so:

- One refresh = ONE `git status --porcelain=v2 --branch -z` call, run in parallel with the base
  lookup and (Working mode) the staged/unstaged `git diff --numstat` for the +/− counts
  (ADR 0007); Against-base mode adds `merge-base` (cached until HEAD or base moves) and one
  numstat diff since it.
- Refresh is triggered by the file watcher (debounced 300 ms) and when the view opens; never polled.
- Diffs are computed per file on demand and cached by (path, mode, mtime/size, HEAD).
- Never run git for workspaces whose Git view is not open, except the cheap status used for
  sidebar markers (at most once per 2 s per workspace).

- **Header:** current branch, upstream, ahead/behind, and the **base** used for comparison.
  Base auto-detects `origin/HEAD` → `main` → `master`; configurable per workspace (menu or
  `mk project update <p> --base develop`).
- **Two modes:**
    - **Working changes:** uncommitted changes grouped Staged / Unstaged / Untracked.
    - **Against base:** everything on this branch vs base, from `git merge-base HEAD <base>` to the
      working tree — like a pull request diff.
- **Changed-file list** (right of the diff, with the header and commit box) with +/− counts;
  selecting one shows its diff.
- **Diff viewer:** unified (side-by-side is backlog), hunk navigation, collapsed unchanged regions, syntax
  highlighting. Diffs > 1 MB show a summary and "Open in Zed".
- **Stage / unstage** per file or per hunk (daemon builds the patch, runs `git apply --cached`).
- **Discard** per file: always confirms and shows what will be lost.
- **Commit:** multi-line message box, Commit button (`Ctrl+Enter`), and ✦: the workspace's
  default agent writes a Conventional Commits message from the staged changes (else the
  unstaged ones) via `ai.generate` (§7); the answer replaces the draft, nothing is committed
  until the user commits. No amend in v1.
- **Fetch / Push:** fetch refreshes the base; push sends the current branch to its upstream only,
  never with force. Pull, rebase, merge and branch switching are backlog.
- **History:** last 50 commits; select one to see its files and diffs.
- **Task changes:** from a Review-column task card, "View changes" opens the Git view with the diff
  from `task.checkpoint` to now (ties into M5).
- **Diff comments → agent:** pick diff lines (click a new-side line, Shift+click extends; as in
  ADR 0006) → comment, reusing the M6 comment system (`context = 'diff'`, anchored to new-side
  lines + quote = the picked lines shown; not in History). Threads are listed in a margin beside
  the diff while the file has any (or lines are picked, a draft is open, or listing failed). "Send to agent" delivers them to the
  task's session (or a chosen one); the agent fixes the code and answers via `mk comment reply`.
  The prompt is the §12 template with `Fix the code in <file>` instead of `Update <file>`,
  `Line numbers are of the current file.` instead of `Only edit this file.`, and
  `code "<quote>"` instead of `passage "<quote>"`. Comments on non-Markdown files are not
  re-anchored when the code changes (backlog); `doc.list` ignores diff comments.
- **Extra tokens:** `diff-add-bg`, `diff-add-text`, `diff-del-bg`, `diff-del-text`,
  `diff-hunk-header` (dark + light) in `theme.rs`. Diff backgrounds stay very subtle.

---

## §13 UI: keyboard shortcuts

All shortcuts live in `<config>/mkrew/keymap.toml`; below are defaults. While a terminal has focus,
only combinations bound in the keymap are intercepted; everything else goes to the agent. Defaults
therefore avoid plain `Ctrl+<letter>` and favor `Ctrl+Shift`/`Alt`, like Windows Terminal.
One exception: plain Ctrl+C copies while terminal text is selected (ADR 0022); without a selection
it reaches the agent as ^C. Anything sent to the agent clears the selection.
On macOS, `Ctrl` becomes `Cmd` for global and tab commands.

| Group | Action | Default |
| --- | --- | --- |
| Global | Command palette | Ctrl+Shift+P |
| Global | Quick open a file of the active workspace | Ctrl+Shift+O |
| Global | Open keymap settings | Ctrl+, |
| Workspace | Jump to workspace 1–9 | Alt+1 … Alt+9 |
| Workspace | Next / previous workspace | Ctrl+Alt+↓ / Ctrl+Alt+↑ |
| Workspace | Add workspace | Ctrl+Shift+K |
| View | Board / Terminals / Docs | Ctrl+Shift+1 / 2 / 3 |
| Inbox | Jump to next item needing attention | Ctrl+Shift+I |
| Task | New task / run selected task | Ctrl+Shift+N / Ctrl+Enter |
| Tabs | New shell tab / new agent tab (pick profile) | Ctrl+Shift+T / Ctrl+Shift+A |
| Tabs | Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Tabs | Jump to tab 1–9 | Ctrl+Alt+1 … Ctrl+Alt+9 |
| Tabs | Rename / close tab | Ctrl+Shift+R / Ctrl+Shift+W |
| Panes | Split right / split down | Alt+Shift+= / Alt+Shift+- |
| Panes | Move focus | Alt+←↑→↓ |
| Panes | Resize | Alt+Shift+←↑→↓ |
| Panes | Zoom / unzoom pane | Ctrl+Shift+M |
| Panes | Close pane (asks if agent running) | Ctrl+Shift+X |
| Terminal | Copy / paste | Ctrl+Shift+C (or Ctrl+C while text is selected) / Ctrl+Shift+V |
| Terminal / Files / Docs | Find in scrollback / in the opened file / filter the Docs list | Ctrl+Shift+F |
| Terminal | Page up / down | Shift+PgUp / Shift+PgDn |
| Docs | Comment on selection | Ctrl+Alt+M |
| Docs | Send comments to agent | Ctrl+Shift+S |
| Docs | Toggle preview / source | Ctrl+Shift+E |
| Zed | Open workspace or current file in Zed | Ctrl+Shift+Z |
| View | Files / Git | Ctrl+Shift+4 / Ctrl+Shift+5 |
| Files | Expand/collapse, move | ← → ↑ ↓ |
| Files | Toggle ignored files | Ctrl+Shift+H |
| Git | Next / previous file | ↓ / ↑ (or j / k) |
| Git | Next / previous hunk | n / p |
| Git | Stage / unstage selected file or hunk | s / u |
| Git | Toggle Working changes / Against base | b |
| Git | Toggle History | h |
| Git | Focus commit message / commit | c / Ctrl+Enter |
| Git | Comment on selected lines | Ctrl+Alt+M |
| Git | Send comments to agent | Ctrl+Shift+S |
| Agent (v2) | Peek the first Inbox entry (↑/↓ cycle, Esc close); hovering a row also peeks | Ctrl+Shift+Space |
| Agent (v2) | Open the prompt composer of the focused agent pane | Ctrl+Shift+Enter |
| Agent (v2) | Send selection to an agent (Files, Git diff, terminal) | Ctrl+Shift+L |
| Agent (v2) | Broadcast, prompts, follow in Zed, turns, fan-out | command palette only |

In Files and Git views the terminal never has focus, so single-key shortcuts are safe there.

Invalid or duplicate bindings: report the error and fall back to defaults for that entry. The
command palette shows each command's shortcut; tooltips show shortcuts too.

---

## §14 UI: visual design system

Goal: simple, beautiful, easy — soft, never rigid. Feel references: Linear, Zed, Raycast.
The terminal is the hero; everything else steps back.

**Principles**

1. **Little chrome, lots of air.** Separate regions with spacing and subtle background shifts, not
   heavy lines. Borders, if any, are 1 px and faint.
2. **Soft.** Moderate corner radius; very light shadows only on floating layers (popover, dialog,
   palette). No saturated colors besides the accent and status colors. Workspace colors
   (ADR 0020) are the one exception: a small dot per workspace.
3. **One accent color** for selection, primary buttons, focus. Status colors only for agent states.
4. **Progressive disclosure.** Secondary actions on cards, pane headers and rows appear on hover/focus.
5. **Gentle motion.** Panels, view switches, drag & drop: 120–180 ms ease-out. Small, slow spinner.
   Respect the OS reduce-motion setting.
6. **Helpful empty states.** One short sentence plus one primary action with its shortcut.
7. **Keyboard = mouse.** Everything clickable has a visible focus ring; tooltips show shortcuts.

**Design tokens** — all in `crates/mkrew-ui/src/theme.rs`. No hard-coded values in components.

| Token | Dark | Light |
| --- | --- | --- |
| `bg` | #16171B | #FAFAF9 |
| `surface` | #1D1E23 | #FFFFFF |
| `surface-raised` | #25262C | #F3F3F1 |
| `border` | #2D2F36 | #E5E5E1 |
| `text` | #E6E7EB | #1D1E22 |
| `text-muted` | #9A9CA6 | #6B6D76 |
| `accent` | #7C8CFF | #4F5BD5 |
| `status-working` | #7C8CFF | #4F5BD5 |
| `status-needs-input` | #E3A63E | #B7791F |
| `status-done` | #4CB782 | #2F855A |
| `status-exited` | #E5676B | #C53030 |
| `status-idle` | #6E7079 | #9A9CA6 |
| `diff-add-bg` | #1E3A2B | #E6F4EA |
| `diff-add-text` | #7DD3A0 | #1E6B3A |
| `diff-del-bg` | #3D2226 | #FDEEEE |
| `diff-del-text` | #F2A0A4 | #A61B1B |
| `diff-hunk-header` | #25262C | #F3F3F1 |

- Spacing scale 4 · 8 · 12 · 16 · 24 · 32 px. Card padding 12, gap between cards 8, main padding 16–24.
- Radius 4 (badges, small buttons), 8 (cards, panes, inputs), 12 (dialogs, palette).
- UI font = system font (Segoe UI Variable / SF / Inter) unless `[ui] font_family` is set. Sizes
  12 meta, 13 body, 15 card title, 20 view title at the default `[ui] font_size = 13`; other sizes
  keep their ratio to body (ADR 0024). Weights 400/500/600. No all-caps text.
- Docs preview (ADR 0021): body 14 px, line height 1.6, headings 24/20/17/15/14/14 px, code 13 px,
  a centered column of at most 720 px.
- Terminal font defaults: Cascadia Code → Cascadia Mono → Consolas (Windows), SF Mono → Menlo
  (macOS), JetBrains Mono → DejaVu Sans Mono (Linux): the first installed one, 13 px;
  `[terminal] font_family` / `font_size` override them (a family that is not installed is
  ignored). Icons of
  Nerd Font prompts/TUIs (private-use code points) come from an installed Nerd Font "Mono"
  family (`theme::TERMINAL_GLYPH_FALLBACKS`), then the system fallback. ANSI palette
  harmonized with the theme.
- Density: one "comfortable" density in v1 (a compact option is backlog).
- Focused pane: faint 1 px accent border; unfocused panes dimmed ~5%.
- Derived tokens (accent mixed into `bg`): `pane-focus-border` #49518D / #A4AAE7;
  `terminal-selection-bg` #3A406B / #CFD2F0 (selected terminal text is drawn in `text`).
  Find matches (`status-needs-input` amber mixed into `bg`, text drawn in `text`):
  `terminal-match-bg` #3F3524 / #F6EBCF; `terminal-match-current-bg` #7A5A1F / #E9C46A.
- Workspace colors (red … pink): dark #E5676B #F0955A #E3C14E #4CB782 #5EC4D6 #7C8CFF #C28CF0
  #F08CC0 / light #C53030 #C05621 #975A16 #2F855A #1F8A9E #4F5BD5 #8A3FB8 #B83280 (≥ 3:1 on
  every surface).
- In-app toasts: bottom-right, auto-hide after 4 s, never blocking.
- Theme follows OS light/dark.

**Design check (every milestone with UI):** capture screenshots of new views in light and dark
into `docs/screenshots/` (if automatic capture is not possible, list them in `docs/HUMAN_CHECKS.md`)
and self-review: no hard-coded colors/spacing; hover/focus/disabled states exist; empty states
exist; body text contrast ≥ 4.5:1; status never conveyed by color alone.

---

## §15 Non-functional requirements

| Area | Requirement |
| --- | --- |
| UI startup | first frame < 500 ms with the daemon running |
| Input latency | key press → glyph on screen < 16 ms (local) |
| Idle CPU | UI + daemon < 1% of the machine with no output (GPUI's Windows vsync floor alone is 1–2% of one core) |
| Memory | UI < 150 MB, daemon < 50 MB with 6 sessions (agents excluded) |
| Security | IPC endpoint accessible by the current user only; no network listeners; `hook.report` rejected unless the session id belongs to a live session |
| Data | SQLite WAL; daily DB backup, keep 7 |
| Errors | panics logged with backtrace; UI shows an error instead of dying silently |
| History | per-session output log, rotated at 5 MB |
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
- [ ] Workspace sidebar; add workspace via folder picker.
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
- [ ] Per-session status correct on pane header, tab label and sidebar (§11); hover shows source.
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
- [ ] Before Run in a git repo: store HEAD in `task.checkpoint`; "Undo task" shows the diff to be
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

- [ ] Files view: lazy tree, respects `.gitignore`, git markers; read-only viewer with syntax
      highlighting; large/binary files handled.
- [ ] Quick open fuzzy-finds files in a 10,000-file repo in < 100 ms.
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
- [ ] Windows installer (MSI or similar) and tar.gz for Linux/macOS.
- [ ] README: install, configure agents, uninstall.
- [ ] Non-functional checks from §15 measured and recorded in `docs/decisions/` or the README.

### M9 — Attention (§20.0, §20A)

- [ ] `settings.toml` (§20.0): defaults when missing, parse errors reported in `daemon.status`
      with the last good settings kept, re-read after the file changes; unit tests.
- [ ] `session.read` returns the last screen lines; the peek popover shows them for an Inbox or
      sidebar row, and its quick reply answers a fake agent's "(y/n)" without leaving the view (e2e
      for the request + manual check for the popover).
- [ ] Turns start and end as §20A.3 defines; a fake agent that edits 2 files in one turn produces
      `session.turn` with both paths (+/− counts arrive in M11); Inbox row and pane header show the summary.
- [ ] `notify.needs_input` / `notify.done` / `notify.sound` switches are respected (unit tests on
      the decision; manual check for the sound).
- [ ] Keep awake: inhibitor held while an agent works and released ≤ 5 s after the last one
      stops (fake inhibitor test); real backends per OS; manual check (`powercfg /requests`).
- [ ] Broadcast sends one text to every checked session; each result is shown.
- [ ] Inbox rows show the last non-blank screen line (§20A.4).

### M10 — Instructing (§20B)

- [ ] Composer: a multi-line send arrives as one bracketed paste; `@` inserts `file_ref`-formatted
      paths; drafts survive switching panes.
- [ ] Send queue: `session.send` returns `{send_id, queued}`, `session.queue` events, `session.unqueue`;
      e2e with the fake agent: a text queued while working and removed is never typed.
- [ ] Prompt library: samples seeded on first run, variables rendered (unit tests), insertable from
      the palette, the composer and the task dialog.
- [ ] Send selection from Files, Git diff and a terminal opens the right session's composer, prefilled.
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
- [ ] Git and Files views work on a task worktree; Merge only when both sides are clean and aborts
      cleanly on conflict; Remove keeps an unmerged branch (real-git tests).
- [ ] Fan-out runs 2–5 agents in their own worktrees; Compare and Pick work; e2e with fake profiles.
- [ ] Usage: fixture transcripts in the current Claude and Codex formats give the expected
      input/output/context tokens, read incrementally; shown on pane header, card and sidebar.
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

**Backlog after v2:** tray icon / taskbar badge; "mark unread" in the Inbox; remote workspaces
over SSH; side-by-side diff; compact density; `gix` git backend; pull/rebase/branch switching and
conflict resolution in-app; GitHub (PR/issue → task); remote control via web/phone; money
estimates from token usage.

---

## §20 v2: working with agents (M9–M11, M13)

Goal: make the daily loop with agents fast — notice who needs you, answer without switching
views, instruct precisely, review and undo per turn, and isolate parallel work when asked.
Inspired by herdr (peek), Zed's agent panel (queue, follow, per-turn checkpoints, keep awake) and
Orca (worktrees, fan-out, usage). §1 non-goals still hold: no editor, no LLM API calls, no cloud,
no mobile app.

Every v2 feature keeps §2: agent differences live in TOML profiles; the daemon owns state; it
works on native Windows; nothing touches the user's branch, index or files without a confirmed
action (ADR 0010).

### §20.0 Shared pieces

**Settings file** `<config>/mkrew/settings.toml` (`Paths::settings_file()`), read by the daemon
at start and re-read after it changes (the daemon watches its own config dir, debounced 300 ms).
Missing file = all defaults. A parse error keeps the last good settings and is reported in
`daemon.status` (`settings_error`) and by the palette like keymap errors. The UI reads it the
same way and re-reads it within 500 ms of a change (fonts apply live, ADR 0024). Palette
`Global: Open settings` creates the commented file if missing and opens it in Zed. Unknown keys
are errors (same rule as profiles). Full shape with defaults:

```toml
[notify]
needs_input = true          # OS notification on needs_input
done = true                 # OS notification on done
sound = true                # notification plays the OS default sound

[awake]
keep_awake = true           # §20A.5

[composer]
file_ref = "@{path}"        # §20B.1; how a picked file is typed; "{path}" alone for agents without @

[zed]
follow_debounce_ms = 1500   # §20C.4

[ui]
font_family = ""            # §14; "" = system UI font; other UI sizes keep their ratio to it
font_size = 13              # px, 8–32

[terminal]
font_family = ""            # §14; "" = first installed of the platform list; use a monospace family
font_size = 13              # px, 8–32; also the Files and Git code views
```

**Turn.** One request→answer cycle of an agent session. A turn **starts** when the session
enters `working` from `starting`, `idle`, `done` or `needs_input` after user input; it **ends** at
the next `done`, `idle`, `needs_input` or `exited`. A `needs_input` → `working` resume (approval)
continues the same turn. Turns are numbered per session from 1. Pure model in
`mkrew_core::turn` (unit-tested on state sequences); the daemon's actor owns one per session.

**Profile keys added in v2** (all optional; unknown keys stay errors):

```toml
[usage]                        # §20E.3
format = "claude_jsonl"        # "claude_jsonl" | "codex_jsonl"; absent = no usage
context_window = 200000        # tokens, for the % shown
```

### §20A Attention (M9)

1. **Peek + quick reply.** Hovering an Inbox or sidebar session row for 300 ms, or `Ctrl+Shift+Space`
   (first Inbox entry; ↑/↓ cycles entries, Esc closes), opens a popover with the last 8 non-blank
   screen lines (monospace plain text, ADR 0012), the state and its age, and a one-line input. Enter sends
   the text with submit (`session.send`); for a `needs_input` session the popover also shows
   buttons `y` and `n` that send that letter with submit. Focus returns where it was. Data comes
   from the new request `session.read` (below), never from attaching.
2. **Sound.** OS notifications (§8) play the OS default sound when `notify.sound` (notify-rust
   `sound_name`, Windows/macOS; Linux hint). `notify.needs_input` / `notify.done` switch each kind
   off. The 30 s per-session limit stays.
3. **Turn summary.** When a turn ends the daemon broadcasts `session.turn` with `n`, `started_at`,
   `ended_at` and `files` = paths attributed to the session during the turn (the hook-reported
   edits and file-watcher batches already attributed in M5, deduplicated, relative). From M11 each
   file also carries `added`/`removed` (numstat between the turn's start and end snapshots).
   The Inbox row of a `done` session shows `✓ done · 3 files · 4m` (no files → `✓ done · 4m`;
   durations as everywhere else, ADR 0011); the pane header shows the same after the status. The summary text is UI-side; the event carries the data.
4. **Last line.** The Inbox row also shows the last non-blank screen line (truncated, muted) —
   taken from `session.read` with `lines = 1` when the entry appears.
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
   terminal or Run task). Send = `session.send` with submit; multi-line text already goes as one bracketed
   paste (§7). Typing `@` opens the quick-open file picker (§12A); the pick inserts
   `composer.file_ref` with `{path}` = the project-relative path, `/` separators, quoted with `"`
   if it contains spaces. Drafts are kept per session in UI memory (lost on UI restart — fine).
   ✦ (shown while the composer has the keyboard and the draft is not blank) sends the draft to
   the session's own agent profile via `ai.generate` (`enhance_prompt`, §7). The agent may read
   the project folder for context. The rewrite replaces the draft, and the user still sends it.
   Typing waits until the answer arrives; picked prompts or files, selections and pastes made
   meanwhile are added after it. A failure keeps the draft and shows why.
   Task create/edit and the Board follow-up input reuse the same multi-line text box.
2. **Send queue visible.** While a send waits (session working), the composer lists queued texts
   (first line each) with ✕ to remove. Daemon: each queued text gets a `send_id`; `session.send`
   returns `{send_id, queued: bool}`; `session.unqueue` removes a not-yet-written text (`not_found`
   once written); event `session.queue` carries the session's current queue after every change
   (an exiting session announces it empty). The pane header shows `⧗ 2 queued`.
3. **Prompt library.** Markdown files in `<config>/mkrew/prompts/*.md`; file stem = name; optional
   first line `# Title`. Variables: `{file}` (current file of Files/Git, project-relative),
   `{selection}` (picked lines, as quoted text), `{task}` (task title), `{project}` (name). An unknown
   or unavailable variable renders as empty and is listed in a muted note under the preview. Pure
   `mkrew_core::prompt::render(template, vars)`. Entry points: palette "Insert prompt…" (into the
   open composer, else opens one on the focused agent pane), a "Prompts" menu in the composer,
   and a "From prompt…" menu in the task dialog. Three samples are seeded on first run if the dir
   is missing: `review.md`, `explain.md`, `fix-tests.md`. Read by the UI directly (config, not state).
4. **Send selection to agent.** `Ctrl+Shift+L` in Files (picked lines), Git diff (picked new-side
   lines) and a terminal (selected text) opens a session picker (default: the task's session for
   Git task changes, else the workspace's most recently active agent session; same list as
   §12's send dialog), then that session's composer prefilled with
   `` `<path>:<a>-<b>` `` + a fenced block of the text (terminal: fenced block only). The user edits
   and sends.
5. **Drop files / paste images.** Dropping files from the OS onto an agent pane (gpui
   `ExternalPaths`) types their absolute paths, each quoted with `"` if needed, separated by spaces,
   with no Enter. `Ctrl+Shift+V` with an image on the clipboard (gpui `ClipboardEntry::Image`)
   saves it through `attachment.save` and types the returned path the same way. Text clipboard
   behaves as before.
6. **Run with…** A task card's menu offers "Run with…" (pick a profile): updates the task's agent,
   then runs it. Only for Todo or Review cards without a live session.

**Protocol (M10):**

| Method / event | Params | Returns / data |
| --- | --- | --- |
| `session.send` (changed) | as before | `send_id`, `queued` |
| `session.unqueue` | `session_id`, `send_id` | ok |
| event `session.queue` | – | `session_id`, `items[]` (`send_id`, `text`) |
| `attachment.save` | `session_id`, `ext` (`png`/`jpg`/`gif`/`webp`), `data` (base64, ≤ 10 MiB) | `path` — `<data>/mkrew/attachments/<session_id>/<ulid>.<ext>` |

Attachments of sessions that ended more than 7 days ago are deleted by the hourly backup check (§8).

### §20C Reviewing per turn (M11)

1. **Turn snapshots.** At each turn start (§20.0) in a git repo, the daemon records a tree of the
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
2. **Restore to before turn n.** Pane header menu "Turns…" lists the session's turns (n, time,
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
5. **Follow in Zed.** Pane header toggle "Follow in Zed" (also a palette command). While on, each
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
   runs in the project folder, saying so in the first output line (ADR 0010).
   - The task card shows `⎇ mkrew/<branch>`. Files and Git views gain a target chooser
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
   `codex_jsonl`: `<CODEX_HOME or ~/.codex>/sessions/**/*<id>*.jsonl` — and reads only appended
   bytes on each turn end. It sums input/output tokens and takes the context size of the newest
   message (field names checked against the CURRENT formats when implementing; fixtures recorded
   from real files). Event `session.usage` (`input`, `output`, `context`, `context_window?`); the pane
   header shows `ctx 62%` (warning color ≥ 80 %), task cards and sidebar workspace rows show total
   tokens. No money estimate. Unknown/changed format → no usage, one warning in the log.

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
