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
- §12A UI: Files (tree and Editor)
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
- Browse project files, view and edit them (a plain editor with find & replace, ADR 0028); see git changes, stage, commit, and compare
  against a base branch (default `main`, configurable per workspace) like Orca; comment on diff
  lines and send the comments to an agent.
- Simple, calm, beautiful UI — soft, not rigid.
- v2 (§20, M9–M11, M13): make the daily loop with agents faster — notice who needs you, answer
  without switching, instruct precisely, review and undo per turn, and isolate parallel work in
  worktrees when asked.

**Non-goals**

- Not an IDE: no LSP, completion or multi-cursor. The Editor view has a plain editor (ADR 0028); diffs are read-only; bigger edits happen in Zed.
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
| Fuzzy matching | `nucleo-matcher` | quick open files (M7) |
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
├── scripts/                  # verify, autobuild, status, package (setup .exe / tar.gz / macOS app)
├── packaging/                # mkrew.iss (Inno Setup 6 setup .exe)
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
`0022_viewed_files.sql` adds table `viewed_file(project_id, worktree_task_id, repo, list, path, hash, marked_at)`, one row per file the user marked as viewed in the Git view (§12B, ADR 0065).

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
| `task.list/create/update/move/delete` | `project_id`, `status`, `position` | Task (`list`: Task[], filters `project_id?`/`status?`; `create` = Todo card, `title`, `prompt`, `agent_profile?` (none = project `default_agent` at run time, stored as `''`), `position?` (after the last Todo card); `update`: `title`/`prompt`/`agent_profile`, absent = unchanged, `null` agent = default; `move`: `status` + `position`; `delete` returns ok, `invalid_state` while a session of the task is not exited, its exited sessions stay as history with `task_id` = NULL — ADR 0004) |
| `task.run` | `task_id`, `cols`, `rows` | `session_id` |
| `task.undo_preview` | `task_id` | `checkpoint`, `fingerprint` (opaque), `changed[]` (tracked files restored), `added[]` (tracked since the checkpoint, deleted), `created[]` (untracked files created since, deleted), `diff` (≤ 1 MiB) + `diff_truncated`; `invalid_state` without a checkpoint (ADR 0005) |
| `task.undo` | `task_id`, `confirm: true`, `created[]` + `fingerprint` (from the preview) | ok — restores the project folder to the checkpoint and deletes the previewed `created` files that are still new; `invalid_request` without `confirm`, `invalid_state` while a session of the task is live, without a checkpoint, or when the tracked changes no longer match `fingerprint` |
| `session.spawn` | `project_id`, `agent_profile?`, `kind`, `tab_id?`, `title?`, `cols`, `rows` | `session_id` — no `agent_profile` = project `default_agent` (agent) or `default_shell` (shell), `invalid_request` when that default is not set; a named profile sets the kind; a tab with `null` layout gets the session as its only pane |
| `session.list` | `project_id?` | Session[] (with `terminal_title?` from daemon memory) |
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
| `session.history` | `session_id` | `files[]`: existing session log files, oldest first (`<id>.log.1`, `<id>.log`); works for exited sessions (also from before a restart); empty after retention deleted the log (ADR 0060) |
| `session.resume` | `session_id`, `tab_id?` | `SessionSpawnResult` — relaunches an exited agent session into its conversation with the profile's `resume` command, as a new session (same title, color, pin, plan, size; a task's session stays the task's, in its worktree); placed where the old pane still is, else into `tab_id` when that tab is empty (the client places it otherwise). `invalid_state` while the session is live, for a shell, without `native_session_id` or `resume`, in an archived workspace, or while a live session has the same `native_session_id` or task (ADR 0055) |
| `session.link_plan` | `session_id`, `path?` (a `.md` file, absolute or project-relative; `null` unlinks) | the Session (also `session.updated`) — live sessions only (`invalid_state`/`not_found` otherwise); `invalid_request` when the file is not `.md`, missing, a folder, over 2 MiB or not UTF-8; the daemon then watches that one file (ADR 0039) |
| `session.plan` | `project_id` | `[{session_id, name, done, total, items[], error?}]` of the project's live sessions with a plan (the last announced checklist, for a client that connects later) |
| `tab.list/create/rename/close/move` | `project_id`, `title`, `position` | TerminalTab (new tabs: `layout` = `null`; close deletes the tab and removes every session with a pane in it, like `session.remove`; no `tab` event, ADR 0041) |
| `tab.pin` | `tab_id`, `pinned` | TerminalTab, also broadcast as `tab.updated`; `position` is left alone, so unpinning puts the tab back |
| `tab.set_layout` | `tab_id`, `layout` | ok |
| `hook.report` | `mkrew_session_id`, `agent` (`claude`/`codex`/`omp`/`copilot`/`fake`), `event`, `payload` | ok — `not_found`/`invalid_state` unless the session is live; stored as a `hook` session event |
| `agent.profiles` | – | AgentProfile[] (each with `installed`: its program, or the OS shell for a shell without `command`, is found on PATH) + parse errors |
| `doc.list` | `project_id` | `DocFile[]` sorted by path: md files matching review globs + md files in `files_changed` session events of the last 24 h + files with unresolved comments (existing files only); each with `open_comments` (unresolved thread roots), `last_session_id?`/`last_changed_at?` (newest such event). Keeps the project watched for `doc.changed` until the daemon stops |
| `doc.read` | `project_id`, `file_path` | `file_path`, `content`, `hash` (opaque); ≤ 2 MiB UTF-8; `invalid_request` for absolute/`..` paths. Also keeps the project watched |
| `comment.list/create/reply/resolve/reopen` | `file_path`, `start_line`, `end_line`, `quote`, `body` | DocComment (every comment carries `context`: `doc` (default) or `diff`, replies = their root's; `list`: `project_id` + `file_path?` + `context?` → DocComment[], roots and replies oldest first; `create`: `project_id`, 1-based inclusive lines, `context?` (`diff` = new-side lines of a diff); `reply`: `comment_id`, `body`, `mkrew_session_id?` (set by `mk comment reply` = agent author); `resolve`/`reopen`: `comment_id`). Ids are `c_` + ULID; the prefix is optional when parsing |
| `comment.send_to_agent` | `comment_ids[]`, `session_id` | the thread roots, now `sent` (`sent_to`, `sent_snapshot` = file content), request order; reply ids stand for their thread; all on one file of the live session's project and of one `context` (`diff` → the prompt asks to "Fix the code in <file>" instead of "Update <file>", without "Only edit this file"). The §12 prompt is queued like `session.send` (submit); multi-line sends are wrapped as a bracketed paste when the program enabled mode 2004 |
| `fs.list` | `project_id`, `dir`, `show_ignored` | children (lazy) with git status |
| `fs.read` | `project_id`, `path`, `index_text?` (bool) | content (≤ 2 MB), binary flag, language, `hash?` (opaque, set with `content`), `index_text?` (asked for: the file's text in the git index, ADR 0047) |
| `fs.write` | `project_id`, `path`, `content`, `expected_hash` | `saved`, `hash` — writes an existing file (≤ 2 MB text, in place) only if it still hashes to `expected_hash`; else `saved: false`, nothing written, `hash` = the disk's (ADR 0028) |
| `fs.find` | `project_id`, `query`, `limit` | fuzzy file matches |
| `git.status` | `project_id`, `mode` (`working`/`base`), `since?` (base mode: compare from this commit instead of the merge-base, e.g. `task.checkpoint`; `merge_base` = it) | `branch?`, `head?`, `upstream?`, `ahead`/`behind`, `base_ref?` + `base_auto`, `merge_base?` (base mode); changed files (`path`, `old_path?`, `kind`, `added?`/`removed?`) in `staged`/`unstaged`/`untracked` (working mode; conflicts are `unstaged`) or `against_base` (base mode) |
| `git.diff` | `project_id`, `path`, `mode`, `staged?` (working mode: index side), `commit?`, `since?` (base mode, as in `git.status`) | `binary`, `too_large`, `added`/`removed`, `hunks[]` (`old/new_start/lines`, `header`, `lines[]` with `kind`, `text`, `old_line?`/`new_line?`, `no_newline`, `crlf`) |
| `git.viewed` | the params of `git.status` | `ViewedFile[]` (`path`, `staged`): the files of that list (`working` / `base` / the `since` checkpoint) whose viewed mark still stands (ADR 0065). Re-hashes the marked files through the diff cache and deletes marks whose diff text changed |
| `git.set_viewed` | the params of `git.diff` (no `commit`), `viewed` (bool) | `null`; hashes the file's diff now and stores (`viewed: true`) or removes the mark; `invalid_state` for a file with no diff, `invalid_request` with `commit` |
| `git.stage` / `git.unstage` | `paths[]` or `hunk` | ok |
| `git.discard` | `paths[]`, `confirm: true` | ok (destructive; UI always confirms first) |
| `git.commit` | `message` | new commit hash |
| `git.log` | `limit`, `path?` | Commit[] |
| `git.show` | `commit` | commit info + changed files |
| `ai.generate` | `generation_id` (client-chosen), `project_id?` (required except for `ask`), `worktree_task_id?`, `agent_profile?` (none = project `default_agent`; required for `ask`), `kind` (`commit_message`/`enhance_prompt`/`ask`), `input`, `web?` (`ask` only, default false) | `null` at once; runs the profile's `oneshot` (§6) with a 120 s limit in the project/worktree folder, prompt on stdin; `ask` runs `ask` (else `oneshot`), or `ask_web` with `web`, in `<data>/mkrew/scratch` with the question as the prompt (ADR 0061); a missing command is `invalid_state` naming the line to add; answer in `ai.generated` |
| `git.set_base` | `project_id`, `base_ref` (`null`/blank = auto-detect; trimmed; refused if it starts with `-` or contains whitespace or control characters) | ok — broadcasts `project.updated` (Project carries `base_ref`) |
| `git.fetch` / `git.push` | `project_id` | ok (push = current branch to its upstream, never force) |
| `git.repos` | `project_id` | `string[]`: `""` when the project folder is in a git repository, then each direct child folder holding `.git` (folder or file), sorted; hidden folders, `node_modules` and `target` skipped; not filtered by `.gitignore`; read on every call (ADR 0029) |
| `daemon.status/stop` | – | info / ok; `status` carries `web` = `{enabled, running, error?}`: the `mk web` child the daemon supervises while `[web] enabled` (§20.0, ADR 0053); `error` = the child's last stderr line |

`git.status`, `git.diff`, `git.stage`/`git.unstage`, `git.discard`, `git.commit`, `git.log`, `git.show`, `git.fetch`/`git.push`, `git.viewed`, `git.set_viewed` and `ai.generate` accept `repo?`: one folder name from `git.repos`; the request runs in that child repository. Paths in requests and replies stay relative to the project folder (`fe/src/a.ts`); a path outside the repo, a bad name or `repo` together with `worktree_task_id` is `invalid_request`. `git.changed` has no repo: clients refresh the shown repo (ADR 0029).

**Events**

| Event | When |
| --- | --- |
| `session.output` | **binary frame** (see below): raw PTY bytes, only to attached clients, batched ≤ every 16 ms |
| `session.snapshot` | only to an attached client whose output queue overflowed: fresh screen snapshot replacing the dropped output (backpressure, below) |
| `session.state` | session state changed (`state`, `source` = hook/screen/daemon, `changed_at`) |
| `session.exited` | process exited, with `exit_code` |
| `session.ports` | a run session's port list grew (`session_id`, `ports`): ports are scanned from its screen text (`http(s)://<host>:<port>`, `port(s)[:=] <n>`), first 4 distinct, never removed while it lives |
| `session.usage` | an agent session's token usage changed at a turn end (§20E.3): `session_id`, `input` (fresh, cache excluded), `output`, `cache_read`, `cache_write`, `context`, `context_window?`, `model?`, `transcript?` |
| `task.updated` / `project.updated` / `tab.updated` | entity changed |
| `session.updated` | a session's title, color, pin, position or plan link changed (`session.rename`, `session.set_color`, `session.pin`, `session.move`, `session.link_plan`): the whole Session |
| `session.terminal_title` | an agent session's terminal title (OSC 0/2) changed: `session_id`, `title?` — leading glyphs stripped, paths ignored, ≤ 60 chars, once per change; memory only (ADR 0031) |
| `session.plan` | the checklist of a linked plan changed (`session_id`, `name` = file name without `.md`, `done`, `total`, `items[]` = first 200 task items `{done, text}` (text cut at 120 chars with `…`), `error?` = why the file cannot be read, counts then 0): sent at once on link and ≤ 1 s after each change of the file (debounced), never twice with the same content; `-`, `*` or `+` items (`[ ]`/`[x]`/`[X]`) outside code fences (ADR 0039) |
| `task.deleted` | task removed (`task_id`, `project_id`; ADR 0004) |
| `conflict.detected` | two sessions edited the same file within 60 s (M5): `project_id`, `path`, `session_ids`, `task_ids` |
| `doc.changed` | a watched md file changed (`project_id`, `file_path`, new `hash`, `null` = deleted); its comments are re-anchored right after (`comment.updated`) |
| `comment.updated` | comment created/replied/state changed (incl. agent replies via CLI) |
| `notification.clicked` | user clicked an OS notification on Linux (UI focuses that pane; Windows uses a `mkrew://` link, §8) |
| `git.changed` | workspace git state changed (debounced, ≤ once per 300 ms) |
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
first, at most the daemon's 200 lines; empty and omitted on the alternate screen), `cursor`
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
  `alacritty_terminal` instance sized to the session, with a SMALL scrollback (200 lines). It is
  used for (a) state detection on the real screen text and (b) instant attach snapshots, which
  carry those 200 lines (ADR 0030).
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
  startup, so each session is resumed at most once. A resumed session keeps its linked plan
  (`plan_path`). A session that exits while the daemon runs
  (a shell or agent that quits, a killed one, a run command) leaves its tab layout; one cut off by
  the daemon stop keeps its pane (ADR 0017, ADR 0033). Any exited agent session can also be
  resumed on request: `session.resume`, History's **Resume**, `mk session resume` (ADR 0055).
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
mk open <project> [file[:line]]        # opens in Zed
mk comment list [--file <path>] [--project P]  # default project: MKREW_SESSION_ID's, else cwd's
mk comment reply <comment-id> "<text>" # used by agents to answer review comments
```

`mk` resolves the current session from `MKREW_SESSION_ID` when relevant (e.g. `comment reply`
records the agent as author). `mk attach` makes Mkrew usable without the UI, including over SSH.

---

## §10 UI: layout, workspaces & terminals

One window. A title bar on top spans both columns (Windows and macOS draw it; Linux keeps the OS
title bar and shows the same bar under it, without window controls; ADR 0034):

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ ◈ ◧ A api ▾ ⎇ master [Terminals│Board│Git 3│Docs│Editor*]  (● 2 running :8080 ▾) ◷ ◐ ▯ ◨ [+ New ▾] ─ □ ✕ │
├──────────────┬────────────────────────────────────────────────────────────────────────┤
│ [⌕ Filter… /]⇲+│ tab strip: [▣ ● tab ✕] [>_ ○ tab ✕] +    (● Working · 4s) 📖 Share ◫ ⋯ │
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
empty things hidden, detail on hover / in menus. There is no status bar.
The right panel (below) is a third column right of Terminals and the Editor while open.

- **Sidebar:** workspaces; expanding one lists its live sessions with status (§11); a session that
  exited leaves the list and its pane leaves its tab (ADR 0033; History keeps it).
  A **filter box** (`⌕ Filter sessions…`, `/` focuses it while no terminal or field has the
  keyboard; `×` inside it clears it while it holds text) matches workspace name, agent and
  session title as you type (every word must match; client-side, not remembered; while it holds
  text ↑/↓ move a highlight over the matching session rows, no wrap, and Enter opens the
  highlighted one, the first by default, or the first workspace when no session matches; with an
  empty box Enter gives the keyboard back to the pane); beside it the
  remove-idle-agents button (only while there are some), collapse / expand all and `+` (add
  workspace); under it a segmented control `All N`, `Working N` and `Needs you N` (N amber while
  above 0) narrows the list (the numbers count the sessions matching the filter text). *Needs
  you* is what the Inbox listed before: needs_input, plus done and not yet viewed (ADR 0036).
  While a filter or chip is on, only workspaces with a match show, open.
  Each workspace is a card (ADR 0062): `card` fill, faint border, radius 8, 8 px between cards,
  a 3 px stripe of the workspace color on its left edge (none without a color). Its header row:
  chevron, colored letter avatar, bold name (card-title size), 📌, then its status: a
  `● 1 working` pill (pulsing dot) while an agent works, else muted `2 idle`, plus amber `? N`
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
  A pane has its own color too: right-click its header → Change color (same palette; ADR 0037).
- **Pin:** the workspace menu, the pane menu (right-click a sidebar session row or a pane header)
  and the tab menu have Pin / Unpin (`project.update` `pinned`, `session.pin`, `tab.pin`, §7). A
  pinned workspace leads the sidebar, a pinned session leads its workspace's list and a pinned tab
  sits at the left of the tab strip; each group keeps its usual order (workspaces by dragged order, then age;
  sessions newest first, tabs by position), and unpinning puts the item back in it. A pinned item
  shows 📌 next to its name (after it in the sidebar, before it on a tab). Pins are stored (§5), so they survive a restart; a resumed session
  keeps its pin (ADR 0017).
- **Reorder:** drag a workspace card in the full sidebar to move it among the pinned or the
  unpinned workspaces (`project.update` `position`, the group numbered `1..=n`; ADR 0069), with
  the same drag look as tabs and rows; off while filtering and in the compact rail.
- **Views per workspace:** Terminals, Board, Git, Docs, Editor (only while a file tab is open; Docs only with `[features] docs`).
- **Title bar** (44 px, ADR 0027, ADR 0034, ADR 0054), left to right: the Mkrew logo; the
  sidebar toggle `◧`; the workspace avatar + name ▾ (a dropdown of the workspaces, click
  switches, plus "Add workspace…") followed, muted, by `⎇ master` (one repository) or
  `⎇ N repos` (several; the only place this shows; the tooltip lists each repository's
  branch and HEAD; a workspace folder without `.git` but with child repositories shows the
  children, ADR 0029; hidden when there is no repository; a click shows the Git view); the
  views Terminals / Board / Git / Docs / Editor as a segmented control (tooltips show their
  shortcuts, §13), the Git one with a neutral gray `N` badge (N = changed files summed over every
  repository, hidden at 0); then, right-aligned, the Run pill, History (`◷`: finished sessions
  with their logs replayed read-only; **Resume** (Enter) continues the selected agent session's
  conversation in a new pane that gets the keyboard, ADR 0055), the theme toggle,
  the phone icon (`smartphone`, §14; a green dot while Phone access is on; the tooltip explains
  it; hidden without a daemon connection), the right panel toggle (◨, ADR 0038, ADR 0081) and the
  outlined `+ New ▾` (one row per installed agent profile, the workspace's default first with ✓
  and Alt+Shift+A, then Shell with Alt+Shift+S; choosing a profile starts a pane with it), in every view (New's entries switch to
  Terminals first; they show Alt+Shift+A / Alt+Shift+S, §13). Every icon is gray (`text-muted`,
  `text` on hover): color only for status. The git label and badge come from `git.repos`
  and one `git.status` per repository, asked for the shown workspace only, on a workspace switch
  and on its `git.changed`; no polling, nothing for hidden workspaces. Window: the empty part is
  the drag area. **Windows:** double-click maximizes or restores, ─ □ ✕ at the right (hovering
  □ shows the Windows 11 Snap Layouts flyout), all edges resize; ✕ and Alt+F4 hide the window to the tray while `[ui] close_to_tray` is on (§20J); the controls are hidden in full
  screen. **macOS:** native traffic lights at the left, drag and double-click as the OS does.
  **Linux:** the OS title bar stays. Minimum window size 1100 × 560. Without a workspace only
  the logo, the sidebar toggle, the drag area and the window controls show.
- **No status bar** (ADR 0054): its sidebar toggle is in the title bar, its Inbox state and
  compact toggle in the sidebar footer, its git HEAD in the title bar's repository tooltip, the
  focused agent's context share in the tab strip (single pane) or pane header (split), and the
  other usage fields (tokens, cache, model) on the Session tab's card.
- **Board:** columns by task status, drag & drop. Card: title, agent, run time, session status.
  Todo cards have Run. Running cards have a **follow-up** input (sends via `session.send`; Esc gives
  the keyboard back and keeps a typed draft on the card, like the composer).
  New task: the empty board's button, or `New task` in the Todo column header.
  With `[jira] site` set, a `Tasks | Jira` switch heads the Board (§20I).
- **Terminals:** tab bar on top; each tab is a split tree of panes; each pane is a session.
  Each tab shows an icon of its focused pane (the agent's badge chip, or `>_` for a shell; none
  for a tab without panes), one status dot of its most urgent pane (§11: pulsing accent while an
  agent works, amber needs input, green done or a live run process, red exited, a hollow ring
  idle), its title and a ✕ (always on the active tab, on hover for the others); closing never
  activates the tab first. `+` (new tab) follows the tabs. At the right end of the strip: for a
  single pane its status (§11) and agent controls, then the split button and `⋯`. The split button opens
  the layout menu: Split right, Split down, Equalize panes, then the layouts 2 columns, 2 rows,
  3 columns, 1 + 2 and 2 × 2, each with a small drawing and its shortcut (§13). Equalize panes
  gives every pane of a run of same-direction splits the same share (split right twice: 50/25/25
  becomes thirds; a stack inside a column counts as one column) and saves the layout. A layout rearranges the active tab:
  its panes keep their tree order (left to right, top to bottom within a column) and the panes it
  lacks start sessions of the focused pane's kind (as Split right), one after another; then the
  layout is saved once. A layout with fewer panes than the tab has is greyed (its tooltip says
  why); if a start fails, the sessions that started are placed as Split right places them.
  `⋯`, and a right-click on a tab, open the tab menu: Pin, Split right, Split down, Rename, Close,
  Close others, Close tabs to the right (both leave pinned tabs and make the menu's tab active;
  greyed when nothing would close); on the active tab also the focused pane's Rename pane, Close
  pane and, for an agent, Follow in Zed (✓ while on) and Turns…. The tab is
  activated first, so the menu acts on the tab it was opened on. Closing a tab stops its panes'
  sessions and removes them from the sidebar (ADR 0041); a running agent among them asks first
  (once for Close others / to the right). Drag a tab along the strip to reorder it (`tab.move`),
  among the pinned or the unpinned tabs only; tabs and sidebar rows share one drag look: a small
  title chip follows the pointer, the item stays dim in place and an accent line marks the gap
  it lands in.
  Sessions are `agent` (from a profile) or `shell` (a shell profile, §6: the workspace's default
  shell, picked in the workspace menu under "Default shell for new panes" like the default agent;
  a new shell pane without one asks the user to pick; ADR 0032).
  New panes open in the project folder. Closing a pane with a running agent asks: kill the
  session, or just hide it (still reachable from the sidebar). A pane whose session exits closes
  itself. Layout persists (`terminal_tab`). Drag the gap between two panes to resize them (each
  side keeps at least 10 % of the split; hidden while a pane is zoomed); the layout is saved once
  when the button is released.
  No hard limit on terminals; warn above 12 running sessions per workspace.
  **Reading width** (agent panes): the pane's 📖 toggle limits the terminal to 960 px (≈ 120–140
  columns), centered, so the agent wraps there: the terminal element is narrower and the usual
  resize sends `session.resize` (nothing reflows its output). Agent panes start with
  `[ui] reading_width` (default `true`); the toggle holds per pane until restart. Shell and run
  panes are always full width.
- **Right panel** (ADR 0038, ADR 0081): a column right of the Terminals and Editor views with
  tabs `Files │ Session` (a click switches the tab). The title bar's `◨` and `Ctrl+Shift+Y` hide
  and show it and keep the tab; from the Board, Docs or Git view the key shows Terminals with the
  panel open. `[ui] right_panel` (`"files"`, `"session"` or `"hidden"`, default hidden) remembers
  it across restarts. Drag its left edge to resize (240–560 px, one width for both tabs, saved as
  `[ui] inspector_width` when the button is released); the terminals lose that width, so an agent
  redraws once. **Files tab:** the file tree of §12A. **Session tab:** the session of the active
  tab's focused pane, also while the Editor is shown.
  **Session card:** agent, title, state + time, the usage of §20E.3 (context
  share, tokens, cache, model; rows without data are left out), **Open transcript** (the agent's
  `.jsonl` in Zed; disabled until the daemon located it) and **Stop** (`session.kill`; a running
  agent asks first, a shell does not; the pane closes when the session exits, ADR 0033).
  **Changes card:** one line per repository with changes, `M 23 ?3` (modified paths, untracked
  files) with **Review diff**, which opens the Git view on that repository (§12B); a workspace
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
  selection & copy, paste (bracketed), scrollback (UI-side, 10,000 lines, only for attached panes),
  search, resize → `session.resize`. Performance rules (mandatory — this is the hottest path):
    - Batch adjacent cells with identical style into one shaped text run per row (as Zed's
      terminal element does). Never one GPUI element per cell.
    - Re-layout only dirty rows; cache shaped rows otherwise.
    - Present at most once per display refresh; coalesce output that arrives between frames.
    - Panes that are not visible (other tab, other workspace, minimized window) are detached:
      no parsing, no layout, no rendering. Re-attach uses the daemon snapshot, so the pane's
      scrollback restarts from the daemon's ≤ 200 history lines (ADR 0030).
    - Use a monospace font with a precomputed cell size; no per-cell measurement.
- **Open in Zed:** on task cards, the workspace menu, the file tree menu and Docs; runs
  `zed <path>[:line]`; clear error with install hint if `zed` is missing.
- **Open in File Explorer:** in the workspace menu and in the file tree's right-click menu
  (§12A). It opens the OS file manager (Explorer, Finder, the Linux file manager): a folder is
  opened, a file is shown selected in its folder.
- **Terminal links:** while Ctrl (Cmd on macOS) is held over a terminal pane (agents and
  shells) the link under the pointer is underlined and the pointer is a hand; Ctrl+Click
  (Cmd+Click) opens it. Links are `http://` / `https://` URLs (opened in the browser) and file
  paths, relative (`./src/a.rs`, `crates/x/main.rs`) or absolute (`/home/u/a.rs`,
  `C:\x\a.rs`), with an optional `:line` or `:line:col`. A path is a link only if it names an
  existing file inside the pane's folder (the project folder, or the worktree of the pane's
  task); it opens in the Editor (§12A) with the caret on that line. Trailing `.,;:!?`,
  unbalanced closing brackets and quotes are not part of a link. `file(12)` (the path before
  the bracket is the link, without the line), `~/…` paths and links wrapped over two rows are
  not detected. Perf: the row text comes from the cached grid, for the hovered row only, while
  the key is held and the pointer moves to another cell; nothing runs while output is parsed
  or painted.
- **Run pill** (ADR 0018, ADR 0054) in the title bar: while something runs a green pill
  `● 2 running :8080 :5180 ▾` (`run-pill-*` tokens; amber dot and `1/2 running` when some
  exited; a steady dot: a pulse would redraw the window for as long as a server runs, §15);
  otherwise a muted `Run ▾`. Ports: every distinct port of the live run sessions, config order,
  up to 4 then `+N`; tooltip `name — :port …` per command. A click opens the Run menu
  (mouse-driven): `RUNNING IN <WORKSPACE>` (`RUN IN` while idle), one row per command (state
  dot, name, ports or state in words, ↻ and a red ■ while it runs, ▶ otherwise), then
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
- **Empty states** everywhere (see §14).

---

## §11 UI: per-session status

Status shows in three places: the tab strip (a single pane) or pane header (a split), the tab
dot, the sidebar row. Always icon + text, never color alone (the tab dot's hover gives the text).

| State | Icon | Extra |
| --- | --- | --- |
| starting | ◌ | |
| working | ••• three dots taking turns, one step per status tick (500 ms) | elapsed time |
| needs_input | ? | highlighted pane border, counted in "Needs you" |
| idle | ○ | idle time |
| done | ✓ | counted in "Needs you" until viewed |
| exited | ✕ | exit code (History); its sidebar row and pane close (ADR 0033) |
| unknown | · | |

- Pane status (ADR 0054), shown at the right end of the tab strip for a single pane (it has no
  header) and in the compact header of each pane of a split: the task folder (only for a task
  that runs in a worktree, shown as its branch without `mkrew/`), **one status pill** (the
  status mark and `Working · 1h 25m`; a done pane adds the last turn, `Done · 3 files · 4m`;
  hovering it shows the source and last change), a conflict warning, `⧗ N queued`, the context
  share, then an agent's Reading width toggle and `Share` menu (for now one item, "Follow in
  Zed", ✓ while on; the button is highlighted while following). Each part shows only when it has
  something to say. "Turns…" is in the tab strip's `⋯` menu. The split header starts with the
  agent badge and the session/task title (F2 or a double-click renames it inline: Enter saves,
  Esc cancels, blank clears; run panes keep their command name — ADR 0031; for a single pane F2
  shows the rename field in the tab's place, and a tab still named `Terminal` / `Terminal N`
  shows that pane's session title instead, cut with `…`, full on hover, a double-click renaming
  the session — a split goes back to the tab name; without a title an agent pane shows the title its
  agent gave the terminal, e.g. Claude's conversation summary; everywhere a session is named the
  order is: user title > terminal title > agent name). The active tab has a 2 px accent line
  along its bottom (inset by its corner radius); in a split the panes keep only their thin
  border, the unfocused ones are dimmed. The sidebar session row also puts the badge
  before the title. The badge is a bordered square chip (a logo chip has no border at rest, one
  color, ADR 0058); it also shows on board cards and in agent menus.
- Pane menu (right-click a split pane's header or a sidebar session row): **Rename** (inline
  like F2; greyed for a run pane), **Mark unread** (a finished agent goes back to Needs you until its pane is focused again, §20J; greyed otherwise) and **Change color** (the workspace palette; none clears). A
  colored pane tints its agent chip and draws a thin line under its header (a single pane: on
  top of its terminal); the border still shows state (ADR 0037).
- Tab label: the most urgent state among its panes: needs_input > done > working > idle > exited.
- Workspace row: `● N working` (a pill, pulsing dot) while an agent works, else `N idle`; amber
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
- **Comments:** pick lines (click a preview block or a source line, Shift+click extends) →
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

## §12A UI: Files (tree and Editor)

- **Tree** (the right panel's Files tab, ADR 0081; opens with `Ctrl+Shift+3`; its rows drag onto
  panes, §20B.5): lazily loaded per folder, respects `.gitignore` (an
  `Ignored` toggle beside the search shows ignored files),
  git status markers next to names (M modified, A added, D deleted, ? untracked), a small dot on
  folders containing changes. In a workspace with child repositories (ADR 0029) the markers come
  from the project folder's repository and from each child repository the listed folder is in
  (all of them when the root is listed). Updates live when agents create/delete/rename files.
- **Editor view:** listed in the view bar only while a file tab is open. A tree click or Enter,
  quick open, a terminal link and "New file…" show it (a tree click keeps the keyboard in the
  tree); `Ctrl+Shift+6` shows it again. Closing the last tab, or switching to another worktree,
  shows Terminals.
- **Editor** (the center Editor view; ADR 0028): the opened file with line numbers and syntax highlighting via
  `syntect` (lazy-loaded syntax set; re-highlighted 150 ms after typing stops). A plain editor:
  click places the caret, Shift+click / drag select; arrows (Ctrl = by word), Home/End
  (Ctrl = file), PgUp/PgDn, Backspace/Delete (Ctrl = word), Enter keeps the indent, Tab, Ctrl+A,
  Ctrl+Z / Ctrl+Y undo/redo, Ctrl+C/X/V. Line endings and the final newline are kept. Unsaved
  changes show `●` before the path; Ctrl+S or "Save" writes the file (`fs.write`). Files > 2 MB
  or binary: a read-only notice and "Open in Zed". A scroll bar on the right shows where the
  view is (only when the file is taller than the view): drag its thumb, or press the track to
  jump there.
- **Tabs** (ADR 0080): every opened file has a tab above the header (pinned tabs first; the
  strip scrolls sideways). A single click (tree, Enter, quick open, a terminal link) opens a
  transient tab (italic) that the next one replaces while it is clean; a double click, an edit,
  Pin or "New file…" keeps it. `●` marks unsaved text; ✕ (on the active tab and on hover), a
  middle click, `Ctrl+W` or `Ctrl+Shift+W` closes a tab, asking "Save / Don't save / Cancel"
  first (Cancel closes nothing); the right neighbour, else the left one, is shown. Right-click:
  Close, Close others, Close to the right, Close all (these three skip pinned tabs), Pin /
  Unpin, Copy path. Tabs are dragged within their group (pinned / unpinned). `Ctrl+Tab` /
  `Ctrl+Shift+Tab` cycle, `Ctrl+Alt+1–9` jump (9 = last). One find bar for the view, searched
  on the shown tab. `git.changed` reads the shown tab; the others are read when shown. Tabs
  live in memory only; another worktree starts with none (and shows Terminals).
- **Change gutter** (ADR 0047): right of each line number a thin bar marks how the line differs
  from the file's text in the git index (what the Git view calls unstaged): added
  (`diff-add-text`), changed (`diff-mod-text`), and a short `diff-del-text` mark at the top of
  the line after removed lines. It follows unsaved typing (recomputed with the highlight, 150 ms
  after typing stops; marks on lines edited since are hidden until then) and staging/committing
  (the `git.changed` re-read). Untracked files, files outside a repository and conflicted files
  have no bars; the bar column is always there so the text never moves.
- Unsaved text is never replaced (§2.5). If the file changes on disk meanwhile (an agent), or
  Save finds another version on disk, a banner "Changed on disk — Reload · Keep mine" offers
  "Reload" (drop your changes) or "Keep mine" (Save overwrites it). Closing a tab, switching
  to another worktree or closing the window with unsaved changes asks "Save / Don't save /
  Cancel" (once per unsaved tab). Switching workspace or view keeps them.
- **Markdown preview:** a `.md` file opens as a rendered preview, by the Docs renderer (§12,
  §14: the same typography tokens for light and dark, highlighted code blocks, ```` ```mermaid ````
  blocks drawn as diagrams, built off the UI thread). The header has a `Preview | Source`
  toggle; Source is the editor above. In Preview (ADR 0072) the text can be selected with the
  mouse across blocks (Shift+click extends, double-click takes a word, Ctrl+A selects the page,
  Esc clears) and Ctrl+C copies it as shown (blocks and lines on their own lines, table cells
  tab-separated, list markers `- ` / `N. ` / `[x] `, drawn diagrams copy nothing; a drag or
  double-click adds no marker to the list item it starts in, items after it keep theirs, and
  Ctrl+A copies every item with its marker). Hovering a top-level block shows ✎ at its top-right corner: its Markdown opens in a
  field in its place; Ctrl+Enter puts it back over the block's source lines (one undo step,
  unsaved like any edit), Esc or a click outside cancels; refused with a toast when the text
  changed since the field opened. The preview is built from
  the editor's text, so unsaved changes show in it and toggling never loses them; it is parsed
  when the file is read and when switching to Preview after an edit, never per keystroke. Find
  and "Open at a line" switch to Source. A new tab starts in Preview. Links in the preview
  are not clickable (ADR 0021). `.md` files also offer "Review in Docs". The page is a virtual
  list (only the blocks on screen are laid out; every block's height is measured once per page)
  with the editor's scroll bar, so long files scroll smoothly; Docs shows its preview the same
  way. A re-read or edited page keeps the scroll position; another tab (or the same file
  opened again) starts at the top, and a live font change measures the page again.
- **Quick open** (`Ctrl+Shift+O`, or the "Search files…" entry at the top of the tree)
  fuzzy-finds files of the current workspace, in the folder the tree shows (project or
  task worktree) (`ignore` to walk, `nucleo-matcher` to rank).
- "Open in Zed" opens the file at the caret's line.
- **Right-click menu** on tree rows (files and folders) and on the blank part of the tree (the
  folder the tree shows: the project, or the task worktree): "New file…" and "New folder…" (a
  name field over the tree; Enter creates it in the folder clicked, or next to the file
  clicked; `/` in the name makes subfolders; a new file opens, a new folder opens in the tree),
  "Copy path" (absolute), "Add to .gitignore" (rows only: appends `/path` or `/path/` to the
  shown folder's `.gitignore`, once), "Open in File Explorer" and "Open in Zed" (§10). The app
  does these on the local disk; the watcher's `git.changed` lists the tree again. Failures show
  in the sidebar. A click outside closes the menu.
- **Open at a line:** Ctrl+Click on a file path in a terminal (§10) opens that file in the Editor with the
  caret on the line, scrolled into view. A path in another task's worktree switches to
  that worktree (asking about unsaved text first).
- **Find & replace** (VS Code style): `Ctrl+F` (also `Ctrl+Shift+F` or "Find" in the header)
  opens the bar, `Ctrl+H` opens it with the replace row. A selection on one line becomes the
  query. Case-insensitive; matches highlighted, Enter / Shift+Enter move between them and select
  the match, Tab / Shift+Tab switch find ↔ replace. In the replace field Enter replaces the
  current match and goes to the next; Ctrl+Alt+Enter (in either field) or "Replace all" replaces
  every match as one undo step. Replacing does not save. Esc closes the bar and returns to the
  editor.

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
- **Changed-file list** (right of the diff, with the header and commit box) with +/− counts;
  selecting one shows its diff.
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
  **Show diff** button. The side panel header shows `Viewed N/M` (M = every file of the shown
  list, whatever "Changed by" and the file filter hide). The daemon keeps each mark in SQLite
  with a hash of the file's diff text; it clears by itself as soon as that text changes or the
  file stops changing (checked with every status refresh). Staging a whole viewed file keeps the
  mark; marks of Against base, a task's changes and Working changes are separate lists.
- **Stage / unstage** per file or per hunk (daemon builds the patch, runs `git apply --cached`).
- **Discard** per file: always confirms and shows what will be lost.
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
  `diff-mod-bg`, `diff-mod-text` (a line changed in place, Split), `diff-hunk-header` (dark + light)
  in `theme.rs`. Diff backgrounds stay very subtle.

---

## §13 UI: keyboard shortcuts

All shortcuts live in `<config>/mkrew/keymap.toml`, which the Settings dialog's Keyboard page writes
(applied at once, ADR 0073); below are defaults. While a terminal has focus,
only combinations bound in the keymap are intercepted; everything else goes to the agent. Defaults
therefore avoid plain `Ctrl+<letter>` and favor `Ctrl+Shift`/`Alt`, like Windows Terminal.
Editor-only commands may use plain `Ctrl+<letter>` (no terminal has focus there, ADR 0028); the
editor's own keys (arrows, Ctrl+Z/Y/A/C/X/V) are not in the keymap.
One exception: plain Ctrl+C copies while terminal text is selected (ADR 0022); without a selection
it reaches the agent as ^C. Anything sent to the agent clears the selection.
Plain Ctrl+V pastes in a terminal pane; ^V reaches the program only when the clipboard holds no
text, image or files (ADR 0026).
Every text field of the app (task title and prompt, composer, follow-up, commit box, run setup,
comments, broadcast, peek reply, quick open, command palette, Ask AI, Settings, the filters and
find bars, the inline rename) edits like a normal text box (ADR 0075): ←/→, Ctrl+←/→ by word,
Home / End, Ctrl+Home / End, ↑/↓ between lines of a multi-line field, Shift to select; Ctrl+A
(select all: the next key replaces it), Backspace / Delete, Ctrl+Backspace / Ctrl+Delete (the
word before / after the caret: blanks, then a run of letters / digits / `_` or of punctuation,
like the Editor), Ctrl+C / Ctrl+X / Ctrl+V, and Ctrl+Z / Ctrl+Y (a typed word is one undo
step). A click puts the caret under the pointer, Shift+click or a drag selects, a double / triple
click selects the word / line. Where a field sits over a list, ↑/↓ move in the list.
Shift+Enter sends `ESC CR` (the Alt+Enter form), which agents read as a new line; Mkrew has no
kitty / win32-input keyboard mode, so this is the one form every agent gets through ConPTY.
On macOS, `Ctrl` becomes `Cmd` for global and tab commands.

| Group | Action | Default |
| --- | --- | --- |
| Global | Command palette | Ctrl+Shift+P |
| Global | Quick open a file of the active workspace | Ctrl+Shift+O |
| Global | Open settings | Ctrl+, |
| Global | Toggle theme (System → Light → Dark) | Ctrl+Shift+J |
| Global | Select theme | Ctrl+Alt+J |
| Global | Toggle full screen | F11 |
| Global | Toggle sidebar | Ctrl+Shift+B |
| Global | Toggle compact sidebar | Ctrl+Alt+B |
| Global | Voice input: start / stop recording (§20G) | Ctrl+Shift+D |
| Global | Ask AI: a question outside every project (§20H) | Ctrl+Shift+G |
| View | Toggle the right panel (Files / Session tabs; keeps the tab) | Ctrl+Shift+Y |
| Workspace | Jump to workspace 1–9 | Alt+1 … Alt+9 |
| Workspace | Next / previous workspace | Ctrl+Alt+↓ / Ctrl+Alt+↑ |
| Workspace | Add workspace (folder or clone) | Ctrl+Shift+K |
| View | Terminals / Board / Git / Docs | Ctrl+Shift+1 / 2 / 4 / 5 |
| View | Show file tree (right panel, the tree gets the keyboard; ADR 0081) | Ctrl+Shift+3 |
| View | Show editor (only while a file tab is open) | Ctrl+Shift+6 |
| Inbox | Jump to next / previous item needing attention (sessions asking for input first, then newest first) | Ctrl+Shift+I / Ctrl+Alt+I |
| Task | New task / run selected task | Ctrl+Shift+N / Ctrl+Enter |
| Board | Select a card (↑/↓ in the column, ←/→ the same row of the next column with cards) / move it one column (the same `task.move`s as a drag) | ↑↓←→ / Shift+← Shift+→ |
| Board | Open the selected card (its live session, else its edit dialog) / edit it | Enter / E |
| Run | Start all / restart all / stop all (§10 Run) | Ctrl+F5 / Ctrl+Shift+F5 / Ctrl+Alt+F5 |
| Tabs | New shell tab / new agent tab (pick profile) | Ctrl+Shift+T / Ctrl+Shift+A |
| Tabs | Next / previous tab (file tabs while the Editor is shown) | Ctrl+Tab / Ctrl+Shift+Tab |
| Tabs | Jump to tab 1–9 (file tabs while the Editor is shown; 9 = the last) | Ctrl+Alt+1 … Ctrl+Alt+9 |
| Tabs | Rename / close tab (close: the active file tab while the Editor is shown) | Ctrl+Shift+R / Ctrl+Shift+W |
| Panes | Split right / split down | Alt+Shift+= / Alt+Shift+- |
| Panes | Layout the active tab: 2 columns / 2 rows / 3 columns / 1 + 2 / 2 × 2 (§10) | Alt+Shift+1 / 2 / 3 / 4 / 5 |
| Panes | Equalize pane sizes (§10) | Alt+Shift+0 |
| Panes | New agent pane / new shell pane (split right with the workspace default, like + New ▾ → Agent / Shell) | Alt+Shift+A / Alt+Shift+S |
| Panes | Move focus | Alt+←↑→↓ |
| Panes | Resize | Alt+Shift+←↑→↓ |
| Panes | Zoom / unzoom pane | Ctrl+Shift+M |
| Panes | Close pane (asks if agent running) | Ctrl+Shift+X |
| Panes | Rename pane (also: double-click its title) | F2 |
| Terminal | Copy / paste | Ctrl+Shift+C (or Ctrl+C while text is selected) / Ctrl+V or Ctrl+Shift+V |
| Terminal / Editor / Docs | Find in scrollback / in the opened file / filter the Docs list | Ctrl+Shift+F |
| Terminal | Page up / down | Shift+PgUp / Shift+PgDn |
| Docs | Comment on selection | Ctrl+Alt+M |
| Docs | Send comments to agent | Ctrl+Shift+S |
| Docs | Toggle preview / source | Ctrl+Shift+E |
| Zed | Open workspace or current file in Zed | Ctrl+Shift+Z |
| File tree | Expand/collapse, move | ← → ↑ ↓ |
| File tree | Toggle ignored files (while the tree has the keyboard; the palette entry works anywhere in a workspace) | Ctrl+Shift+H |
| Editor | Save the opened file | Ctrl+S |
| Editor | Find / replace in the opened file (replace all: Ctrl+Alt+Enter in the bar) | Ctrl+F / Ctrl+H |
| Editor | Close the active file tab (asks about unsaved text; ADR 0080) | Ctrl+W |
| Git | Next / previous file | ↓ / ↑ (or j / k) |
| Git | Next / previous hunk | n / p |
| Git | Stage / unstage selected file or hunk | s / u |
| Git | Toggle Working changes / Against base | b |
| Git | Toggle History | h |
| Git | Toggle Unified / Split diff | d |
| Git | Mark the selected file viewed / not viewed | v |
| Git | Focus commit message / commit | c / Ctrl+Enter |
| Git | Stage all of the selected file's group (Unstaged + Untracked when a Staged file or nothing is selected) / unstage all | Shift+S / Shift+U |
| Git | Discard the selected file (asks first) / fetch / push | x / f / Shift+P |
| Git | Comment on selected lines | Ctrl+Alt+M |
| Git | Send comments to agent | Ctrl+Shift+S |
| Agent (v2) | Peek the first session that needs you (↑/↓ cycle, Alt+Y / Alt+N answer, Ctrl+O open the pane, Esc close); hovering a row also peeks | Ctrl+Shift+Space |
| Agent (v2) | Open the prompt composer of the focused agent pane | Ctrl+Shift+Enter |
| Agent (v2) | Send selection to an agent (Editor, Git diff, terminal) | Ctrl+Shift+L |
| Agent (v2) | Broadcast, prompts, follow in Zed, turns, fan-out, new pane of a chosen agent profile ("Agent: New <Profile> pane") | command palette only |
| Task / Workspace | Merge back / remove worktree of the selected task; pin, open in File Explorer, set default agent, remove, workspace menu of the active workspace | command palette only |

In the Editor and Git views the terminal never has focus, so single-key shortcuts are safe there.
Alt+Shift+A / Alt+Shift+S and F2 no longer reach a focused terminal (Alt+Shift+A / S used to be
ESC A / ESC S, a readline binding); rebind `split_agent` / `split_shell` / `rename_pane` in
Settings → Keyboard if needed.

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
   (ADR 0020) and pane colors (ADR 0037) are the exception: a small tinted letter avatar per
   workspace and a 3 px stripe on its sidebar card, and a tinted agent chip with a thin line
   under the header of a colored pane.
3. **One accent color** for selection, primary buttons, focus. Status colors only for agent states.
4. **Progressive disclosure.** Secondary actions on cards, pane headers and rows appear on hover/focus.
5. **Gentle motion.** Panels, view switches, drag & drop: 120–180 ms ease-out. Small, slow working dots.
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
| `status-needs-input` (attention) | #E4A452 | #A4660F |
| `status-done` (success) | #57C78A | #23854F |
| `status-exited` (danger) | #EF7B6C | #C2412F |
| `status-idle` | #7D838C | #8A8F99 |
| `diff-add-bg` / `diff-del-bg` | #182422 / #261D20 | #E7EFEC / #F5E9E9 |
| `diff-add-word-bg` / `diff-del-word-bg` | #254638 / #543331 | #C4DDD0 / #ECCCC8 |
| `diff-add-text` / `diff-del-text` | #6FD39B / #EF8F82 | #1E6B3A / #A61B1B |
| `diff-hunk-header` | #17191D | #F1F2F4 |
| `qr-dark` | #101114 | #101114 |
| `qr-light` | #FFFFFF | #FFFFFF |

Tints (chip and pill fills) are a color at ~14 % (dark) / ~12 % (light) alpha.

- Spacing scale 4 · 8 · 12 · 16 · 24 · 32 px. Card padding 12, gap between cards 8, main padding 16–24.
- Radius 5 (area chips, key hints), 6 (chips, segments, tree rows), 8 (buttons, inputs, rows),
  9 (segmented container), 10 (cards), 12 (dialogs, palette), 999 (pills); 4 for old badges.
- Heights: controls 30, inputs 32, title bar 48, tab strip 44, Git repo strip 42 (px at 13 px body).
- Shared controls (`shell/widgets.rs`): segmented control, primary / secondary / ghost / destructive
  buttons, 30 px icon button, search box, status pill, tinted chip, overline, key hint.
- UI font = Inter when installed, else the system font (Segoe UI Variable / SF), unless
  `[ui] font_family` is set (Inter is not bundled). Sizes 11 overline, 12 meta, 13 body, 15 card
  title, 20 view title at the default `[ui] font_size = 13`; other sizes keep their ratio to body
  (ADR 0024). Weights 400/500/600. All-caps only for 11/600 overlines.
- Docs preview (ADR 0021): body 14 px, line height 1.6, headings 24/20/17/15/14/14 px, code 13 px,
  a centered column of at most 720 px.
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
  shape wins); a lone pane has no border, borders mark the focused split pane and needs-input.
  Editor code rows keep line height 1.35 (`code_line_height`).
- Icons (ADR 0051): icon-only buttons, toggles, window controls, row actions and row chevrons
  draw Lucide SVG icons (`icon/lucide/`, `crates/mkrew-ui/src/icons.rs`) in a theme token color,
  16 px in icon buttons and 14 px among text (`ICON_SIZE`, `ICON_SIZE_SMALL`). The glyphs this
  spec names for those buttons map to Lucide: `+` plus, `▾ ▸ ‹ ›` chevrons, `⋯` ellipsis, `◷`
  history, `⌫` delete, `◐ ☀ ☾` sun-moon / sun / moon, `─ □ ❐ ✕` minus / square / copy / x, `⌕`
  search, `◫` columns-2, `◧ ◨` panel-left / panel-right, `⇤ ⇥` panel-left-close / -open,
  `↻ ■ ▶` rotate-cw / circle-stop / play, `✦` sparkles, Fetch `↓` arrow-down-to-line. Close
  pane is an icon-only `square-x` in the tab strip. Frequent toolbar, header, card and
  empty-state actions carry a 14 px icon before their text (Share, `+ New`, idle Run, Save, Find,
  Comment, Send to agent, Open in Zed, Push, …; the list is in ADR 0051); a `▾` after a label
  is a chevron-down icon. Dialog footers, Retry, thread actions, row actions and danger buttons
  stay text. Status icons (§11), marks in messages and the live Run label stay text.
- Phone access (ADR 0053): a Lucide `smartphone` icon button in the title bar, right of the
  theme toggle (accent while `mk web` runs, `text-muted` otherwise; tooltip "Phone access: on" /
  "Phone access: off"), and the palette command "Global: Phone access…", open the "Phone
  access" dialog (`DIALOG_WIDTH`, same frame as the other dialogs, Esc / Close). Rows, each
  with a glyph and text (§11): **Status** `○ Off` + Start, `● On` + Stop, `✕ Not running:
  <error>. Retrying…` + Stop; **Tailscale** `✓ Tailscale serve is set up`, `Tailscale doesn't
  publish Mkrew yet` + Set up (its error below), `Install and sign in to Tailscale first`; **Link**
  (only while On and the MagicDNS name is known): the QR code, the link as one line and Copy
  link, "Scan with the iPhone camera, then Share → Add to Home Screen." (`Starting…` until
  `mk web` has written `web_token`). The QR is always dark modules (`qr-dark`) on a light
  panel (`qr-light`) in both themes, `PHONE_QR_SIZE` px. The dialog re-reads `daemon.status`
  and Tailscale about once a second while open (off the UI thread); the title bar icon reads
  `daemon.status` every 5 s. A failed Start / Stop shows in the dialog. The link carries the
  access token: it is never logged and never in a tooltip.
- Settings (Ctrl+,, palette "Global: Open settings", ADR 0073): a modal dialog (`SETTINGS_DIALOG_WIDTH` ×
  `SETTINGS_DIALOG_HEIGHT`, never larger than the window minus a margin); page list on the left
  (General, Appearance, Notifications, Voice, Jira, Phone, Features, Keyboard), rows on the right
  (label, help, control: switch, segmented choice, text field, list pick (fonts, themes)); Number and Text rows are
  written on Enter or when the row loses the keyboard, Esc drops the draft. Keyboard rows show a
  chip, an accent dot when changed and Reset; a click or Enter captures the next keystroke, Esc
  cancels. A refused value or duplicate keystroke shows under its row, never as a toast. A file that
  does not parse shows a banner with "Fix in Zed" and the settings controls do nothing. The footer
  opens `settings.toml` and `keymap.toml` in Zed.
- Density: one "comfortable" density in v1 (a compact option is backlog).
- Focused pane: faint 1 px accent border; unfocused panes dimmed ~5%.
- Derived tokens (accent mixed into `bg`): `pane-focus-border` #49518D / #A4AAE7;
  `terminal-selection-bg` #3A406B / #CFD2F0 (selected terminal text is drawn in `text`).
  Find matches (`status-needs-input` amber mixed into `bg`, text drawn in `text`):
  `terminal-match-bg` #3F3524 / #F6EBCF; `terminal-match-current-bg` #7A5A1F / #E9C46A.
- Workspace colors (red, orange, yellow, lime, green, teal, blue, purple, pink): dark #F87171
  #FB923C #FACC15 #A3E635 #4ADE80 #2DD4BF #60A5FA #C084FC #F472B6 / light #DC2626 #D9560B
  #A16207 #4D7C0F #15803D #0F766E #2563EB #9333EA #DB2777 (≥ 3:1 on every surface; neighbouring
  hues ≥ 12° apart).
- Update button (§20K): Lucide `circle-arrow-up` in `accent-text` in the title bar right before
  the phone icon while a newer release is known; dimmed (`opacity::DISABLED`) and not clickable
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
  dialog's error row, the commit box, the tab bar notice, a card notice) stays there.
- Confirmations (`window.prompt`) are drawn in the window, never as the OS message box
  (`App::set_prompt_builder`, `shell/confirm_prompt.rs`): the modal shade, a `surface` card
  (`CONFIRM_DIALOG_WIDTH`, radius 12, faint border, light shadow), the question as a card
  title, the detail in `text-muted`, buttons on the right with Cancel leftmost and the first
  answer rightmost (accent; the danger button for `PromptLevel::Critical`). Esc = Cancel,
  Enter/Space = the ringed button (default: the first answer), Tab/←/→ move the ring. A click
  on the shade does nothing. OS file/folder pickers stay native.
- Theme: System (follows the OS live), Light or Dark — `[ui] theme`, cycled by the title bar toggle
  (ADR 0027). Each slot shows a built-in theme (ADR 0078): `[ui] light_theme` (default Mkrew Light;
  One Light, GitHub Light, Catppuccin Latte, Solarized Light, Gruvbox Light) and `[ui] dark_theme`
  (default Mkrew Dark; One Dark, Dracula, Catppuccin Mocha, Tokyo Night, Gruvbox Dark); an unknown
  name shows the Mkrew theme. "Theme: Select theme…" (Ctrl+Alt+J) previews while moving, Enter or a
  click on a row saves to the slot shown, Esc restores (an outside click does not close it).
  Settings → Appearance has "Light mode theme" / "Dark mode theme" rows. In terminals the default
  fg/bg and ANSI 0–15 follow the theme; 256-color 16–255 and 24-bit colors are drawn as the program
  sends them.

**Design check (every milestone with UI):** capture screenshots of new views in light and dark
into `docs/screenshots/` (if automatic capture is not possible, list them in `docs/HUMAN_CHECKS.md`)
and self-review: no hard-coded colors/spacing; hover/focus/disabled states exist; empty states
exist; body text contrast ≥ 4.5:1; status never conveyed by color alone; run action glyphs ≥ 3:1.

---

## §15 Non-functional requirements

| Area | Requirement |
| --- | --- |
| UI startup | first frame < 500 ms with the daemon running |
| Input latency | key press → glyph on screen < 16 ms (local) |
| Idle CPU | UI + daemon < 1% of the machine with no output (GPUI's Windows vsync floor alone is 1–2% of one core) |
| Memory | UI < 150 MB, daemon < 50 MB with 6 sessions (agents excluded) |
| Security | IPC endpoint accessible by the current user only; the daemon has no network listener (`mk web` — started by the user, or by the daemon as a child while the user turned phone access on, ADR 0053 — listens on 127.0.0.1 by default and requires its token on every WebSocket, ADR 0052); with `[web] push = true` `mk web` also POSTs encrypted Web Push messages to the push service host (Apple, Google, Mozilla, Microsoft only) of each subscribed phone, ADR 0067; `hook.report` rejected unless the session id belongs to a live session |
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
      `session.turn` with both paths (+/− counts arrive in M11); the pane header shows the summary.
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
- [ ] Send selection from the Editor, Git diff and a terminal opens the right session's composer, prefilled.
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

**Backlog after v2:** remote workspaces
over SSH; compact density; `gix` git backend; pull/rebase/branch switching and
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
flash_taskbar = true        # §20J; Windows: the taskbar button flashes while a session needs input and the window is not focused

[awake]
keep_awake = true           # §20A.5

[composer]
file_ref = "@{path}"        # §20B.1; how a picked file is typed; "{path}" alone for agents without @

[zed]
follow_debounce_ms = 1500   # §20C.4

[ui]
font_family = ""            # §14; "" = system UI font; other UI sizes keep their ratio to it
font_size = 13              # px, 8–32
theme = "system"            # §14; "system" | "light" | "dark"; the title bar toggle writes it
light_theme = "Mkrew Light" # §14; a built-in theme name, shown in Light mode / a light OS
dark_theme = "Mkrew Dark"   # §14; shown in Dark mode / a dark OS
diff_mode = "unified"       # §12B; "unified" | "split"; the Git diff header toggle writes it
inspector_width = 300       # §10; px, the right panel's width (dragging its edge writes it; kept 240–560)
right_panel = "hidden"      # §10; "files" | "session" | "hidden"; the right panel's toggle and tabs write it
sidebar_width = 280         # §10; px, the workspace sidebar's width (dragging its edge writes it; kept 200–480)
close_to_tray = true        # §20J; Windows: ✕ hides the window to the tray icon (false: ✕ quits the UI)

[terminal]
font_family = ""            # §14; "" = first installed of the platform list; use a monospace family
font_size = 13              # px, 8–32; also the Editor and Git code views

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
   The pane header of a `done` session shows `✓ Done · 3 files · 4m` (no files → `✓ Done · 4m`;
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
   first line `# Title`. Variables: `{file}` (current file of the Editor/Git, project-relative),
   `{selection}` (picked lines, as quoted text), `{task}` (task title), `{project}` (name). An unknown
   or unavailable variable renders as empty and is listed in a muted note under the preview. Pure
   `mkrew_core::prompt::render(template, vars)`. Entry points: palette "Insert prompt…" (into the
   open composer, else opens one on the focused agent pane), a "Prompts" menu in the composer,
   and a "From prompt…" menu in the task dialog. Three samples are seeded on first run if the dir
   is missing: `review.md`, `explain.md`, `fix-tests.md`. Read by the UI directly (config, not state).
4. **Send selection to agent.** `Ctrl+Shift+L` in the Editor (the editor's selected lines, else the caret's line), Git diff (picked new-side
   lines) and a terminal (selected text) opens a session picker (default: the task's session for
   Git task changes, else the workspace's most recently active agent session; same list as
   §12's send dialog), then that session's composer prefilled with
   `` `<path>:<a>-<b>` `` + a fenced block of the text (terminal: fenced block only). The user edits
   and sends.
5. **Drop files / paste images.** Dropping files from the OS onto an agent pane (gpui
   `ExternalPaths`) types their absolute paths, each quoted with `"` if needed, separated by spaces,
   with no Enter. The whole pane accepts the drop (header, terminal, composer). While the pane's
   composer is shown, the paths go into its draft instead of the terminal. `Ctrl+V` /
   `Ctrl+Shift+V` with an image on the clipboard (gpui `ClipboardEntry::Image`; a BMP image is
   re-encoded as PNG) saves it through `attachment.save` and types the returned path the same
   way; copied files type their paths. Text clipboard behaves as before.
   **Drag from the file tree.** Dragging a row of the file tree (§12A) onto a pane types
   `composer.file_ref` with the path relative to the tree's folder (the project, or the task
   worktree) on an agent pane, the bare path on a shell pane, a folder ending with `/`, then one
   space; no Enter. The same rules as above: the whole pane accepts it, the paths go into the
   composer draft while it is shown, and the pane shows an accent ring while the row hovers it.
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
5. **Follow in Zed.** Pane header `Share` menu → "Follow in Zed" (also a palette command). While on, each
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
   `cache_write`, `context`, `context_window?`, `model?`, `transcript?` (the located transcript file's absolute path, sent again when it changes)); the pane header shows `ctx 62%` (warning
   color ≥ 80 %), task cards show total tokens (the workspace row shows a session summary, ADR 0036). No money estimate.
   omp (per API call, summed): `message.usage` `input` (already fresh), `cacheRead`, `cacheWrite`,
   `output` of `type: "message"` assistant lines; context = `input` + `cacheRead` + `cacheWrite` of the
   newest call; `model` = `message.model` of the newest call (`model_change` lines are not
   trusted). A call with no tokens (aborted) changes neither. omp's file does not hold the
   context window: `context_window` in the profile is the only source.
   Unknown/changed format → no usage, one warning in the log.

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
   content. It fades and slides in over `theme::motion::SLOW`. It holds a breathing dot, a
   seven-bar waveform, `m:ss`, and `Ctrl+Shift+D stops`. The waveform follows the microphone
   loudness, smoothed: quick up, slow down. A wave runs across the bars, and in silence they keep a
   low breath, so a live microphone never looks still. It is redrawn every
   `theme::VOICE_METER_TICK`. While transcribing, the pill shows the `working` dots and
   `Transcribing`, and after that it disappears. It never takes a click or the keyboard.
   The field or terminal pane that has the keyboard gets an accent ring over its border. The ring
   breathes while recording and holds steady while transcribing. Under reduce motion, the dots,
   the ring, and the wave stand still, and the bars still follow the loudness.
   The sidebar footer (the title bar while the sidebar is hidden, and the rail) shows only the
   breathing dot or the `working` dots. Errors and notes (`✕ Voice: …`) show there and stay until
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
   "Loading Jira issues…" with working dots. A row: issue type, key, summary (one line, cut with
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
   dialog opens; no second task is made. Click a row without one: the row shows working dots
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

1. **Taskbar badge (Windows).** The main window's taskbar button shows an overlay icon with that
   count (`ITaskbarList3::SetOverlayIcon`; amber, `9+` from 10; description "3 need you");
   cleared at 0. macOS and Linux: nothing (gpui has no dock-badge call).
2. **Flash (Windows).** When a session enters `needs_input` while the window is not focused, the
   taskbar button flashes until the window is focused (`FlashWindowEx`); it stops on focus and when
   nothing needs you any more. `[notify] flash_taskbar = false` turns it off (applies live).
3. **Tray icon (Windows).** Owned by the UI process, shown while it runs. Icon: Mkrew's own while
   nothing works or waits, a dot in the working color while an agent works, a dot in the needs-input
   color while something needs you; tooltip `Mkrew · 2 need you · 1 working`. Right-click menu: up to
   8 sessions that need you (asking for input first, then newest first), `agent · workspace — state, age` (a click shows the window
   and opens that pane); a greyed `● N working`; `Open Mkrew`; `Quit UI (agents keep running)`. A
   left click opens the window.
4. **Close to tray (Windows).** `[ui] close_to_tray = true` (default): ✕ and Alt+F4 hide the window
   instead of closing it (no pane is attached while hidden, §10). The tray menu, an OS notification click and
   starting Mkrew again (same exe, same daemon socket) show it. `false`, or no tray icon: ✕ quits the UI as before
   (unsaved Editor changes ask first). `Quit UI` shows the window if unsaved Editor changes need an
   answer, then ends the UI; the daemon and every agent keep running (§2.2).
5. **Mark unread.** Pane menu (right-click a split pane's header or a sidebar session row) **Mark
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
   `mkrew-<v>-macos-arm64.zip` (ad-hoc signed `Mkrew.app`) and the Linux tar.gz.
2. **Check.** The UI asks `https://api.github.com/repos/tungmk97/mkrew/releases/latest` at window
   open and every 6 h (10 s timeout, background executor), and on palette "Global: Check for
   updates" (no key). Only a plain `a.b.c` tag strictly greater than the running version counts.
   404, network or parse errors show nothing (one log line); the palette check answers with a
   toast ("… is up to date", "… is available", or the error). A newer version shows the update
   button (§14) and one info toast "Mkrew <v> is available".
3. **Install kind.** Windows: `<exe dir>/../unins000.exe` exists → Inno setup, per-user when the
   exe is under `%LOCALAPPDATA%`. macOS: the exe is `<X>.app/Contents/MacOS/mkrew` → app bundle.
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
