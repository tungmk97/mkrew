# Mkrew

A lightweight native desktop app to dispatch tasks to many coding agents (Claude Code, Codex,
Oh My Pi, …) across many projects, watch their state, and know instantly which agent needs you.
Reading and editing code happens in [Zed](https://zed.dev); Mkrew only orchestrates.

Mkrew has three programs that must sit in the same folder:

| Program | What |
| --- | --- |
| `mkrew` | the desktop app |
| `mkrew-daemon` | runs the agents (PTYs, state, database); started by `mkrew`, or with `mk daemon start` |
| `mk` | command-line client; agent hooks call it too, so it must be on your `PATH` |

Agents keep running when you close the window: they live in the daemon.

## Install

You need the agents you want to use (`claude`, `codex`, `omp`) installed and logged in, and
optionally Zed with its `zed` command on your `PATH` (for "Open in Zed").

### Windows (MSI)

Run `mkrew-<version>-x64.msi`. It installs to `C:\Program Files\Mkrew\` (`bin\` with the
three programs, `agents\` with reference copies of the sample profiles), adds `bin\` to the
system `PATH` and creates a Start Menu shortcut "Mkrew". Open a new terminal afterwards so the
new `PATH` applies.

### Linux / macOS (tar.gz)

```bash
tar -xzf mkrew-<version>-<os>-<arch>.tar.gz
mkdir -p ~/.local/opt && mv mkrew-<version>-<os>-<arch> ~/.local/opt/mkrew
# Add the bin folder to PATH (put this line in ~/.bashrc, ~/.zshrc, …):
export PATH="$HOME/.local/opt/mkrew/bin:$PATH"
mkrew
```

The archive holds `bin/{mkrew,mk,mkrew-daemon}`, `agents/*.toml` (reference copies of the sample
profiles) and this README.

### From source

Needs stable Rust (selected by `rust-toolchain.toml`). Windows: the MSVC toolchain and
Visual Studio Build Tools with "Desktop development with C++". Linux: the system libraries GPUI
needs (the same ones as building Zed).

```bash
cargo install --locked --path crates/mkrew-ui       # mkrew
cargo install --locked --path crates/mkrew-cli      # mk
cargo install --locked --path crates/mkrew-daemon   # mkrew-daemon
```

All three land in `~/.cargo/bin`, which rustup already puts on your `PATH`. Release packages:
`bash scripts/package.sh` (tar.gz) or `pwsh scripts/package.ps1` (MSI, needs WiX 7).

## Configure agents

### Agent profiles

Each agent is a TOML file in the agents folder of the config directory (see
[Files and folders](#files-and-folders)); the file name is the profile id. On first start the
daemon writes the samples `claude.toml`, `codex.toml`, `omp.toml`, `shell.toml` and
`fake.toml` (a fake agent for tests) there. It never overwrites a file you edited, but recreates
a deleted sample on every start. Profiles are read each time a session starts, so edits apply to
the next session.

A new agent needs only a new file, e.g. `aider.toml`:

```toml
name = "Aider"
command = "aider"
args = []
prompt_mode = "type"     # "arg": prompt as last argument · "type": typed in when ready · "none"
env = {}
resume = ""              # e.g. "claude --resume {session_id}"; empty = no resume

[state]
mode = "screen"          # "hook" | "screen" | "hook+screen"
idle_after_ms = 3000
needs_input_patterns = ['(?i)\(y/n\)']
```

Unknown keys and invalid regexes are errors. `kind = "shell"` makes a plain shell profile
(omit `command` to use `%COMSPEC%` / `$SHELL`). The full format is in
[SPEC §6](docs/SPEC.md#6-agent-profiles--agent-state).

### Hooks (exact status for Claude Code and Codex)

Without hooks Mkrew guesses the state from the screen. With hooks the agent reports "working",
"needs input" and "done" itself:

```bash
mk integration install claude    # adds hooks to ~/.claude/settings.json
mk integration install codex     # sets notify in ~/.codex/config.toml
mk integration status claude
```

`CLAUDE_CONFIG_DIR` and `CODEX_HOME` move those files as usual. Install backs the file up
(timestamped `.bak`), merges instead of overwriting and can run again safely. Codex runs only
one `notify` program, so install refuses when `notify` is already set to another one; remove it
first. The hooks call `mk hook …`, which does nothing when the agent runs outside Mkrew.

### Workspaces, default agent and dev server

- Add a project folder as a workspace: "Add workspace" in the sidebar (`Ctrl+Shift+K`) or
  `mk project add <path> [--name N] [--dev "npm run dev"]`.
- Default agent: in the workspace menu pick "Default agent for new panes". New agent panes and
  tasks without their own agent use it.
- Dev server: set the command in the workspace menu → "Dev server settings…" or with
  `mk project update <project> --dev "npm run dev"` (`--no-dev` clears it, `--dev-autostart on`
  starts it whenever the daemon starts). It runs in its own "dev" tab; start or stop it from
  the workspace menu, the command palette or `mk project dev <project> [--stop]`.

## Daily use

- **Sidebar**: workspaces with each session's status; `Ctrl+Shift+I` jumps to the next session
  that needs you.
- **Terminals** (`Ctrl+Shift+2`): tabs split into panes, each an agent or a shell. New agent tab
  `Ctrl+Shift+A`, shell tab `Ctrl+Shift+T`, split `Alt+Shift+=` / `Alt+Shift+-`.
- **Task board** (`Ctrl+Shift+1`): new task `Ctrl+Shift+N`, run it `Ctrl+Enter`.
- **Docs** (`Ctrl+Shift+3`): review Markdown plans, comment on passages and send the comments
  back to the agent session that wrote them.
- **Files** (`Ctrl+Shift+4`) and **Git** (`Ctrl+Shift+5`): browse files, diff against the base
  branch, stage, commit, comment on diff lines. Quick open `Ctrl+Shift+O`; "Open in Zed"
  `Ctrl+Shift+Z`.
- **Command palette** `Ctrl+Shift+P`: every command, plus actions without a shortcut.
- **Shortcuts**: `Ctrl+,` creates `keymap.toml` in the config directory (every command listed,
  commented out, at its default) and opens it in Zed. Edits apply after restarting Mkrew;
  duplicate or invalid entries are listed in the command palette. Defaults:
  [SPEC §13](docs/SPEC.md#13-ui-keyboard-shortcuts).
- **Without the UI** (also over SSH):

  ```bash
  mk daemon start                      # the app starts it for you; without the UI, start it here
  mk session list
  mk attach <session-id>               # detach with Ctrl+]
  mk task add <project> "Fix the flaky test" --prompt "Fix tests/flaky.rs" --agent claude
  mk task run <task-id>
  mk daemon status                     # also prints the data directory
  ```

  All commands: `mk --help` and [SPEC §9](docs/SPEC.md#9-cli-mk).

## Uninstall

1. Remove the agent hooks (only Mkrew's entries are removed; a `.bak` copy is kept):

   ```bash
   mk integration uninstall claude
   mk integration uninstall codex
   ```

2. Stop the daemon (this ends running agent sessions): `mk daemon stop`.
3. Remove the programs:
   - Windows: Settings → Apps → Installed apps → Mkrew → Uninstall (removes the files, the
     `PATH` entry and the shortcut).
   - tar.gz: delete the extracted folder and its `PATH` line.
   - From source: `cargo uninstall mkrew-ui mkrew-cli mkrew-daemon`.
4. Optionally delete your data and settings (below).

## Files and folders

| | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Config (`agents/`, `keymap.toml`) | `%APPDATA%\mkrew\` | `~/.config/mkrew/` | `~/Library/Application Support/mkrew/` |
| Data (`mkrew.db`, `logs/`, `sessions/`, `backups/`) | `%APPDATA%\mkrew\` | `~/.local/share/mkrew/` | `~/Library/Application Support/mkrew/` |

`MKREW_CONFIG_DIR` and `MKREW_DATA_DIR` override them. The daemon listens on the named pipe
`\\.\pipe\mkrew-<username>` (Windows) or `$XDG_RUNTIME_DIR/mkrew.sock` (else in the data
directory).

## Performance

Measured on Windows 11 with release builds (`bash scripts/measure.sh`, Windows:
`pwsh scripts/measure.ps1`); details in
[`docs/decisions/0009-non-functional-measurements.md`](docs/decisions/0009-non-functional-measurements.md).

| | Target | Measured |
| --- | --- | --- |
| Window first frame (daemon running) | < 500 ms | 370 ms |
| Idle CPU, 6 sessions open | < 1% of the machine | 0.14% of 16 threads: UI ~2% of one core (an empty GPUI window alone is 1.25–2%), daemon 0% |
| Memory, 6 sessions open | UI < 150 MB, daemon < 50 MB | UI 60–109 MiB, daemon 19–24 MiB |
| 6 panes flooding output | input < 16 ms, flat RAM, daemon < 1 core | p95 0.5 ms round trip / 8.5 ms per output batch, flat RAM, 0.8 core |
| Git status, 10,000-file repo | < 150 ms | 98 ms |

## Development

The full spec is [`docs/SPEC.md`](docs/SPEC.md). This repository is built by coding agents, one
task per session: see [`docs/BUILD_KIT.md`](docs/BUILD_KIT.md) for how the loop works and
`AGENTS.md` for the conventions. Checks: `bash scripts/verify.sh` (Windows:
`pwsh scripts/verify.ps1`).
