# Resume omp, Copilot and free agent panes after a restart — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After a reboot or `mk daemon stop`, Oh My Pi, GitHub Copilot CLI, Claude Code and Codex conversations come back in their old panes, also for agent panes that are not tied to a task (the herdr behaviour).

**Architecture:** Resume already works for Claude/Codex task sessions (`crates/mkrew-daemon/src/server/resume.rs`). It needs two inputs: the agent's own session id (`session.native_session_id`, set from hook payloads) and a profile `resume` command. This plan (1) teaches `mkrew_core::hook` two new hook agents, `omp` and `copilot`. (2) Ships `agents/omp.toml` with `resume` and a new `agents/copilot.toml`. (3) Extends `mk integration install` with an omp extension file and a Copilot hooks file. Both call `mk hook <agent> …`. (4) Marks sessions the daemon stop cut off (`session.interrupted`, migration 0010). (5) Resumes those that have no task and are still shown in a tab, in the same pane.

**Tech Stack:** Rust workspace, tokio, rusqlite 0.40 (bundled SQLite, `RETURNING` supported), serde_json (`preserve_order`), toml_edit. Tests: `cargo test`, daemon end-to-end tests with `mkrew-fakeagent` (`--resume <id>` prints `Resumed: <id>`).

**Spec:** `docs/SPEC.md` §3 (lifecycle), §5 (schema), §6 (profiles, hook integrations), §8 (resume), §9 (CLI). This plan comes from the user's request of 2026-10-07: "keep omp/claude/codex/copilot sessions as they are when the computer restarts or the app is closed, like herdr". Investigation facts, all checked on this machine:

- `omp --help` (v18.4.4): `-r, --resume=<value>  Resume a session (by ID prefix, path, …)`. omp extensions in `~/.omp/agent/extensions/*.ts` get `session_start`, `session_switch`, `agent_start`, `agent_end` (`event.willContinue`), `tool_approval_requested`/`resolved`, `tool_execution_start`/`end`, and `ctx.sessionManager.getSessionId()` (`omp://extensions.md`, and herdr's own `~/.omp/agent/extensions/herdr-omp-agent-state.ts`). omp sets `OMPCODE=1` in every shell it spawns, so a nested `omp` must not report. Task subagents have `ctx.hasUI !== true`.
- `copilot --help` (1.0.64): `-r, --resume[=value]  Resume from a previous session (… session ID …)`. Copilot hooks (`https://docs.github.com/en/copilot/reference/hooks-reference`): user-level `*.json` files in `%USERPROFILE%\.copilot\hooks\` (or `$COPILOT_HOME/hooks/`), `{"version": 1, "hooks": {…}}`. A command entry may use `"exec"` + `"args"` (no shell), and the payload arrives as JSON on stdin. camelCase events carry `sessionId`: `sessionStart`, `userPromptSubmitted`, `agentStop` (the main agent finished a turn), and `notification` (`notification_type` `permission_prompt` / `elicitation_dialog` / …, with an optional `matcher` regex on it).
- Today `HookAgent::parse("omp")` is `None` (`crates/mkrew-core/src/hook.rs:227`), `agents/omp.toml` has no `resume`, and `resume_tasks` only resumes sessions that have a task (`resume.rs:35-47`).

## Global Constraints

- "An agent is just a terminal command. No per-agent code in the core. Every agent difference lives in its TOML profile." (SPEC §2.1). Hook event mapping in `mkrew_core::hook` is the existing, accepted exception (Claude/Codex already live there).
- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths.
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why. `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network. Use `mkrew-fakeagent`.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change (Task 6: ADR 0017).
- Integration install must back up the original file (timestamped `.bak`), merge or own exactly one file, be idempotent, and have a working uninstall (SPEC §6).
- No protocol change: `PROTOCOL_VERSION` stays 24 (`interrupted` is a store column only, never sent).
- Store gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms). Bind the result first.
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use.
- Leave the existing uncommitted human edits alone (`.github/workflows/*`, `Cargo.lock`, `crates/mkrew-ui/Cargo.toml`, `docs/BUILD_VI.md`).
- Verify with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run `cargo fmt --all -- --check`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace -j 4`.
- "Access is denied" on an `.exe`: rename it to `*.locked-old.exe`. Never kill the user's sessions.

## Review Focus

1. **The user's existing `%APPDATA%\mkrew\agents\omp.toml` has no `resume` line, and `mode = "screen"`.** Seeding never overwrites, so a shipped fix alone changes nothing on this machine. `mk integration install omp` must print the exact line to add. Pinned by Task 3 `resume_hint_names_the_missing_line`.
2. **A nested `omp` (started by omp's own bash tool) or an omp task subagent reports its short-lived session id over the pane's root session.** The pane would then resume the wrong conversation. The extension skips `OMPCODE=1` and `ctx.hasUI !== true`. This is a manual check (Task 6, HUMAN_CHECKS), because no test may run real omp.
3. **A free pane the user closed or hid before the restart comes back anyway.** Only sessions still in a tab layout are resumed. Pinned by Task 5 `restart_resumes_free_agent_panes_in_tabs_once` (the hidden session stays exited).
4. **Restarting twice resumes the old pane twice, or never resumes the new one.** `take_interrupted_sessions` clears the mark, and the resumed session is interrupted again by the next stop. Pinned by Task 4 `interrupted_sessions_are_taken_once` and Task 5 (the second restart).
5. **An agent session the user killed or that exited by itself counts as interrupted.** Only exits during `Hub::shutdown` (`stopping`) and rows left non-exited by a crash or reboot are marked. Pinned by Task 4 (an ended session is not taken) and Task 5 (the hidden session is never resumed).

---

## File Structure

| File | Change |
|---|---|
| `crates/mkrew-core/src/hook.rs` | `HookAgent::{Omp, Copilot}`: parse, event mapping, native id key |
| `crates/mkrew-cli/src/hook.rs` | `mk hook copilot <event>` reads stdin JSON like Claude |
| `agents/omp.toml` | `resume`, `mode = "hook+screen"` |
| `agents/copilot.toml` | new profile |
| `crates/mkrew-daemon/src/lib.rs` | seed `copilot.toml` |
| `crates/mkrew-cli/src/integration.rs` | `Agent::{Omp, Copilot}`: one owned file each; resume hint |
| `crates/mkrew-cli/assets/omp-extension.ts` | new: omp extension calling `mk hook omp …` |
| `crates/mkrew-cli/assets/copilot-hooks.json` | new: Copilot hooks calling `mk hook copilot …` |
| `crates/mkrew-store/migrations/0010_session_interrupted.sql` | new column |
| `crates/mkrew-store/src/lib.rs` | `mark_interrupted`, `take_interrupted_sessions`, stale rows interrupted |
| `crates/mkrew-daemon/src/session.rs` | `Actor::finish` marks shutdown exits interrupted |
| `crates/mkrew-daemon/src/server/resume.rs` | `resume_sessions`: tasks + free panes in tabs |
| `crates/mkrew-daemon/src/server.rs` | call `resume_sessions` |
| `docs/SPEC.md`, `docs/decisions/0017-resume-free-panes-omp-copilot.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` | docs |

---

### Task 1: Hook agents `omp` and `copilot`

**Files:**
- Modify: `crates/mkrew-core/src/hook.rs:14-60,107-115,225-229`
- Modify: `crates/mkrew-cli/src/hook.rs:1-7,63-73,111-133`
- Modify: `crates/mkrew-daemon/tests/daemon.rs:882` (`"omp"` is now a known agent)

**Interfaces:**
- Produces: `HookAgent::Omp` (`"omp"`; events `busy`/`needs_input`/`done`/`session_start`; id key `session_id`) and `HookAgent::Copilot` (`"copilot"`; events `sessionStart`/`userPromptSubmitted`/`agentStop`/`notification`; id key `sessionId`). `mk hook omp <event> [json]` (JSON as argument) and `mk hook copilot <event>` (JSON on stdin). Tasks 3 and 6 rely on these exact names.

- [ ] **Step 1: Write the failing tests** in `crates/mkrew-core/src/hook.rs` `mod tests`. Replace `unknown_agents_are_rejected` and add:

```rust
    #[test]
    fn omp_events_map_like_the_fake_agent() {
        let omp = HookAgent::parse("omp").unwrap();
        assert_eq!(omp.event("busy", &Value::Null), Some(HookEvent::Busy));
        assert_eq!(omp.event("needs_input", &Value::Null), Some(HookEvent::NeedsInput));
        assert_eq!(omp.event("done", &Value::Null), Some(HookEvent::Done));
        assert_eq!(omp.event("session_start", &Value::Null), None);
        let start = json!({"session_id": "01a11668-3523"});
        assert_eq!(omp.native_session_id(&start).as_deref(), Some("01a11668-3523"));
    }

    #[test]
    fn copilot_events_and_session_id() {
        let copilot = HookAgent::parse("copilot").unwrap();
        let none = Value::Null;
        assert_eq!(copilot.event("sessionStart", &none), None);
        assert_eq!(copilot.event("userPromptSubmitted", &none), Some(HookEvent::Busy));
        assert_eq!(copilot.event("agentStop", &none), Some(HookEvent::Done));
        let of = |kind: &str| copilot.event("notification", &json!({"notification_type": kind}));
        assert_eq!(of("permission_prompt"), Some(HookEvent::NeedsInput));
        assert_eq!(of("elicitation_dialog"), Some(HookEvent::NeedsInput));
        assert_eq!(of("shell_completed"), None);
        assert_eq!(of("agent_idle"), None);
        let start = json!({"sessionId": "0cb916db-26aa", "source": "resume"});
        assert_eq!(copilot.native_session_id(&start).as_deref(), Some("0cb916db-26aa"));
        assert_eq!(copilot.native_session_id(&json!({"session_id": "x"})), None);
    }

    #[test]
    fn unknown_agents_are_rejected() {
        assert_eq!(HookAgent::parse("gemini"), None);
        assert_eq!(HookAgent::parse("Claude"), None);
    }
```

In `crates/mkrew-cli/src/hook.rs` `mod tests` add:

```rust
    #[test]
    fn copilot_reads_json_from_stdin() {
        let stdin = || Ok(r#"{"sessionId": "c1", "source": "startup"}"#.into());
        let (event, payload) =
            event_and_payload("copilot", &args(&["sessionStart"]), stdin).unwrap();
        assert_eq!(event, "sessionStart");
        assert_eq!(payload["sessionId"], "c1");
        assert!(event_and_payload("copilot", &args(&["agentStop", "{}"]), no_stdin).is_err());
    }

    #[test]
    fn omp_takes_event_and_json_argument() {
        let (event, payload) =
            event_and_payload("omp", &args(&["busy", r#"{"session_id": "o1"}"#]), no_stdin)
                .unwrap();
        assert_eq!((event.as_str(), payload), ("busy", json!({"session_id": "o1"})));
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-core hook:: ; cargo test -p mkrew-cli hook::`
Expected: FAIL (`parse("omp")` is `None`; copilot `agentStop` with a JSON argument is accepted).

- [ ] **Step 3: Implement** in `crates/mkrew-core/src/hook.rs`:

```rust
pub enum HookAgent {
    Claude,
    Codex,
    /// GitHub Copilot CLI: `~/.copilot/hooks/mkrew.json` (camelCase events, JSON on stdin).
    Copilot,
    /// Oh My Pi: the `mkrew.ts` extension reports `busy`/`needs_input`/`done`/`session_start`.
    Omp,
    /// The test fake agent (SPEC §16).
    Fake,
}
```

`parse` gains `"copilot" => Some(Self::Copilot)` and `"omp" => Some(Self::Omp)`. In `event`, replace the three `Self::Fake` arms and add the Copilot arms:

```rust
            (Self::Copilot, "userPromptSubmitted") => Some(HookEvent::Busy),
            (Self::Copilot, "notification") => copilot_notification(payload),
            (Self::Copilot, "agentStop") => Some(HookEvent::Done),
            (Self::Omp | Self::Fake, "busy") => Some(HookEvent::Busy),
            (Self::Omp | Self::Fake, "needs_input") => Some(HookEvent::NeedsInput),
            (Self::Omp | Self::Fake, "done") => Some(HookEvent::Done),
```

`native_session_id` keys:

```rust
        let key = match self {
            Self::Claude | Self::Omp | Self::Fake => "session_id",
            Self::Codex => "thread-id",
            Self::Copilot => "sessionId",
        };
```

Below `claude_notification`:

```rust
/// Copilot's `notification` hook: tool approvals and questions need the user; shell and
/// background-agent completions are informational.
fn copilot_notification(payload: &Value) -> Option<HookEvent> {
    match payload.get("notification_type").and_then(Value::as_str) {
        Some("permission_prompt" | "elicitation_dialog") => Some(HookEvent::NeedsInput),
        _ => None,
    }
}
```

In `crates/mkrew-cli/src/hook.rs` change the Claude arm to `("claude" | "copilot", [event]) => {`. Change the last JSON arm guard to `if !matches!(agent, "claude" | "copilot")`. Update the module doc:

```rust
//! - `claude <event>` / `copilot <event>`: the hook JSON comes on stdin.
//! - `codex <json>`: Codex `notify` appends the JSON as the last argument; the event is its `type`.
//! - anything else (`omp`, `fake`): `<event> [json]`.
```

In `crates/mkrew-daemon/tests/daemon.rs:882` change `client.hook(id, "omp", "busy", Value::Null)` to `client.hook(id, "gemini", "busy", Value::Null)` and rename the binding `omp` → `unknown_agent`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-core hook:: ; cargo test -p mkrew-cli hook::`
Expected: PASS.

- [ ] **Step 5: Suggested message** (do not commit): `feat(core): omp and copilot hook agents`

---

### Task 2: Shipped `omp` and `copilot` profiles

**Files:**
- Modify: `agents/omp.toml`
- Create: `agents/copilot.toml`
- Modify: `crates/mkrew-daemon/src/lib.rs:81-87`
- Modify: `crates/mkrew-core/src/profile.rs:597`, `crates/mkrew-daemon/tests/daemon.rs:739`
- Modify: `crates/mkrew-core/src/state/tests.rs:329` (comment only)

**Interfaces:**
- Produces: profile ids `omp` (`resume = "omp --resume {session_id}"`) and `copilot` (`resume = "copilot --resume={session_id}"`), both `mode = "hook+screen"`.

- [ ] **Step 1: Write the failing test** in `crates/mkrew-core/src/profile.rs` `repo_sample_profiles_parse`. Change the id list and add the resume checks:

```rust
        assert_eq!(ids, ["claude", "codex", "copilot", "fake", "omp", "shell"]);
        let resumed = |id: &str| set.get(id).unwrap().resumed("abc").map(|p| (p.command, p.args));
        assert_eq!(resumed("omp"), Some(("omp".into(), vec!["--resume".into(), "abc".into()])));
        assert_eq!(resumed("copilot"), Some(("copilot".into(), vec!["--resume=abc".into()])));
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-core repo_sample_profiles_parse`
Expected: FAIL (no `copilot`; omp `resumed` is `None`).

- [ ] **Step 3: Implement.** Write `agents/omp.toml`:

```toml
# Oh My Pi (SPEC §6). State and resume come from `mk integration install omp`.
name = "Oh My Pi"
command = "omp"
prompt_mode = "type"
# Resume command; {session_id} = the id omp reports through the Mkrew extension.
resume = "omp --resume {session_id}"
oneshot = "omp -p --no-session"  # one-shot text; prompt on stdin

[state]
mode = "hook+screen"
idle_after_ms = 3000
needs_input_patterns = []
```

Create `agents/copilot.toml`:

```toml
# GitHub Copilot CLI (SPEC §6). State and resume come from `mk integration install copilot`.
name = "GitHub Copilot"
command = "copilot"
# The prompt is typed once the TUI is ready (`copilot` takes no positional prompt).
prompt_mode = "type"
resume = "copilot --resume={session_id}"
# No `oneshot`: `copilot -p` takes the prompt as an argument, not on stdin.

[state]
mode = "hook+screen"
idle_after_ms = 3000
needs_input_patterns = []
```

In `crates/mkrew-daemon/src/lib.rs` `SAMPLE_PROFILES`, after `codex.toml`, add:

```rust
    ("copilot.toml", include_str!("../../../agents/copilot.toml")),
```

In `crates/mkrew-daemon/tests/daemon.rs:739` change the ids to `["claude", "codex", "copilot", "fake", "omp", "shell"]`. In `crates/mkrew-core/src/state/tests.rs:329` change the comment to `// The shipped profile with no hook yet: screen detection decides, no patterns, idle after 3 s (SPEC §6).` The behaviour is unchanged: in `hook+screen` with no hook events, screen detection applies.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-core profile ; cargo test -p mkrew-core omp_profile ; cargo test -p mkrew-daemon --test daemon agent_profiles_lists_samples_and_broken_files`. Also run the daemon test that spawns the shipped omp profile (the one at `daemon.rs:1105`; find its name with `grep -n "let omp = include_str" -B40 crates/mkrew-daemon/tests/daemon.rs | grep "^.*fn "`).
Expected: PASS.

- [ ] **Step 5: Suggested message:** `feat(profiles): resumable omp and new copilot profile`

---

### Task 3: `mk integration install|uninstall|status omp|copilot`

**Files:**
- Create: `crates/mkrew-cli/assets/omp-extension.ts`
- Create: `crates/mkrew-cli/assets/copilot-hooks.json`
- Modify: `crates/mkrew-cli/src/integration.rs:1-132` (+ tests)

**Interfaces:**
- Consumes: Task 1 hook names (`mk hook omp <event> <json>`, `mk hook copilot <event>` + stdin).
- Produces: `Agent::Omp` → `<PI_CODING_AGENT_DIR or ~/.omp/agent>/extensions/mkrew.ts`; `Agent::Copilot` → `<COPILOT_HOME or ~/.copilot>/hooks/mkrew.json`. Mkrew owns that one file: install writes it whole, uninstall deletes it.

- [ ] **Step 1: Create the assets.** `crates/mkrew-cli/assets/omp-extension.ts`:

```ts
// Installed by `mk integration install omp` (Mkrew). Reinstalling overwrites this file;
// `mk integration uninstall omp` deletes it. Put your own extensions beside it.
// @ts-nocheck
import { execFile } from "node:child_process";

// Only inside a Mkrew pane, and never for an `omp` started by omp's own shell (OMPCODE=1):
// that nested session would report its id over the pane's root session.
const enabled = !!process.env.MKREW_SESSION_ID && process.env.OMPCODE !== "1";

let queue = Promise.resolve();

// One `mk hook omp <event> <json>` at a time, in order; failures are ignored (daemon down).
function report(event, ctx) {
  let sessionId;
  try {
    const id = ctx?.sessionManager?.getSessionId?.();
    sessionId = typeof id === "string" && id.length > 0 ? id : undefined;
  } catch {}
  const payload = JSON.stringify(sessionId ? { session_id: sessionId } : {});
  queue = queue.then(
    () =>
      new Promise((done) => {
        execFile("mk", ["hook", "omp", event, payload], { windowsHide: true, timeout: 10000 }, () => done());
      }),
  );
}

export default function (pi) {
  if (!enabled) return;
  // The pane's root session is the one with a UI; task subagents have none.
  let root = false;
  let waiting = 0;
  const on = (name, handler) =>
    pi.on(name, (event, ctx) => {
      if (!root) {
        if (ctx?.hasUI !== true) return;
        root = true;
      }
      handler(event, ctx);
    });
  const ask = (ctx) => {
    waiting += 1;
    report("needs_input", ctx);
  };
  const answered = (ctx) => {
    waiting = Math.max(0, waiting - 1);
    if (waiting === 0) report("busy", ctx);
  };

  on("session_start", (_event, ctx) => report("session_start", ctx));
  on("session_switch", (_event, ctx) => report("session_start", ctx));
  on("agent_start", (_event, ctx) => report("busy", ctx));
  on("agent_end", (event, ctx) => {
    if (event?.willContinue !== true) report("done", ctx);
  });
  on("tool_approval_requested", (_event, ctx) => ask(ctx));
  on("tool_approval_resolved", (_event, ctx) => answered(ctx));
  on("tool_execution_start", (event, ctx) => {
    if (event?.toolName === "ask") ask(ctx);
  });
  on("tool_execution_end", (event, ctx) => {
    if (event?.toolName === "ask") answered(ctx);
  });
}
```

`crates/mkrew-cli/assets/copilot-hooks.json`:

```json
{
  "version": 1,
  "hooks": {
    "sessionStart": [{ "type": "command", "exec": "mk", "args": ["hook", "copilot", "sessionStart"], "timeoutSec": 10 }],
    "userPromptSubmitted": [{ "type": "command", "exec": "mk", "args": ["hook", "copilot", "userPromptSubmitted"], "timeoutSec": 10 }],
    "agentStop": [{ "type": "command", "exec": "mk", "args": ["hook", "copilot", "agentStop"], "timeoutSec": 10 }],
    "notification": [{ "type": "command", "exec": "mk", "args": ["hook", "copilot", "notification"], "matcher": "permission_prompt|elicitation_dialog", "timeoutSec": 10 }]
  }
}
```

- [ ] **Step 2: Write the failing tests** in `integration.rs` `mod tests`:

```rust
    #[test]
    fn config_files_of_owned_integrations() {
        let none = |_: &str| None;
        let home = directories::BaseDirs::new().unwrap().home_dir().to_owned();
        assert_eq!(
            config_file(Agent::Omp, none).unwrap(),
            home.join(".omp/agent/extensions/mkrew.ts")
        );
        let var = |key: &str| (key == "PI_CODING_AGENT_DIR").then(|| PathBuf::from("/o"));
        assert_eq!(config_file(Agent::Omp, var).unwrap(), Path::new("/o/extensions/mkrew.ts"));
        let var = |key: &str| (key == "COPILOT_HOME").then(|| PathBuf::from("/c"));
        assert_eq!(config_file(Agent::Copilot, var).unwrap(), Path::new("/c/hooks/mkrew.json"));
    }

    #[test]
    fn owned_files_install_replace_and_uninstall() {
        let dir = tempfile::tempdir().unwrap();
        for agent in [Agent::Omp, Agent::Copilot] {
            let file = dir.path().join(format!("{agent:?}")).join("mkrew.x");
            let ours = owned_contents(agent).unwrap();
            assert_eq!(owned_status(None, ours), Installed::No);
            run_on(Action::Install, agent, &file).unwrap();
            let text = fs::read_to_string(&file).unwrap();
            assert_eq!(owned_status(Some(&text), ours), Installed::Yes);
            run_on(Action::Install, agent, &file).unwrap();
            let backups = || {
                fs::read_dir(file.parent().unwrap())
                    .unwrap()
                    .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".bak"))
                    .count()
            };
            assert_eq!(backups(), 0, "an unchanged file is neither written nor backed up");
            // An older version is outdated, replaced, and kept as a backup.
            fs::write(&file, "old").unwrap();
            assert_eq!(owned_status(Some("old"), ours), Installed::Partly);
            run_on(Action::Install, agent, &file).unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), ours);
            assert_eq!(backups(), 1);
            run_on(Action::Uninstall, agent, &file).unwrap();
            assert!(!file.exists());
            run_on(Action::Uninstall, agent, &file).unwrap();
        }
    }

    #[test]
    fn copilot_hooks_call_mk_for_the_events_mkrew_maps() {
        let hooks: Value = serde_json::from_str(COPILOT_HOOKS).unwrap();
        assert_eq!(hooks["version"], 1);
        for (event, entries) in hooks["hooks"].as_object().unwrap() {
            assert_eq!(entries[0]["exec"], "mk");
            assert_eq!(entries[0]["args"], json!(["hook", "copilot", event]));
            let known = mkrew_core::HookAgent::Copilot.event(event, &json!({"notification_type": "permission_prompt"}));
            assert!(known.is_some() || event == "sessionStart", "{event} maps to nothing");
        }
    }

    #[test]
    fn resume_hint_names_the_missing_line() {
        let file = Path::new("C:/cfg/agents/omp.toml");
        let old = "name = \"Oh My Pi\"\ncommand = \"omp\"\n[state]\nmode = \"screen\"\n";
        let hint = resume_hint(Agent::Omp, file, Some(old)).unwrap();
        assert!(hint.contains("resume = \"omp --resume {session_id}\""), "{hint}");
        assert!(hint.contains("omp.toml"), "{hint}");
        let new = include_str!("../../../agents/omp.toml");
        assert_eq!(resume_hint(Agent::Omp, file, Some(new)), None);
        assert_eq!(resume_hint(Agent::Omp, file, None), None, "seeded on the next daemon start");
    }
```

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p mkrew-cli integration::`
Expected: FAIL (no `Agent::Omp`, `owned_contents`, `owned_status`, `resume_hint`).

- [ ] **Step 4: Implement** in `integration.rs`.

Module doc, add two lines:

```rust
//! - Oh My Pi: `<PI_CODING_AGENT_DIR or ~/.omp/agent>/extensions/mkrew.ts`, a Mkrew-owned file.
//! - GitHub Copilot CLI: `<COPILOT_HOME or ~/.copilot>/hooks/mkrew.json`, a Mkrew-owned file.
```

Constants and enum:

```rust
const OMP_EXTENSION: &str = include_str!("../assets/omp-extension.ts");
const COPILOT_HOOKS: &str = include_str!("../assets/copilot-hooks.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Agent {
    Claude,
    Codex,
    Omp,
    Copilot,
}
```

`config_file` match:

```rust
    let (env, dir, name) = match agent {
        Agent::Claude => ("CLAUDE_CONFIG_DIR", ".claude", "settings.json"),
        Agent::Codex => ("CODEX_HOME", ".codex", "config.toml"),
        Agent::Omp => ("PI_CODING_AGENT_DIR", ".omp/agent", "extensions/mkrew.ts"),
        Agent::Copilot => ("COPILOT_HOME", ".copilot", "hooks/mkrew.json"),
    };
```

Add `fn name(agent: Agent) -> &'static str` returning `"claude"`/`"codex"`/`"omp"`/`"copilot"`, and use it in `run_on` instead of the local match. Pull the status printing out of `run_on` (current lines 86-102) into:

```rust
fn print_status(name: &str, file: &Path, installed: &Installed) -> ExitCode {
    let label = match installed {
        Installed::Yes => "installed",
        Installed::Partly => "partly installed (run `mk integration install` again)",
        Installed::No => "not installed",
    };
    println!("{name}: {label} ({})", file.display());
    if mkrew_core::lookup::find("mk".as_ref()).is_none() {
        println!("warning: `mk` is not on PATH ({}); hooks will fail", mkrew_core::lookup::SEARCHED);
    }
    if *installed == Installed::Yes { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
```

At the top of `run_on`, right after reading `text`:

```rust
    if let Some(ours) = owned_contents(agent) {
        return run_owned(action, name(agent), file, text.as_deref(), ours);
    }
```

The Claude/Codex `Status`/`Edit` matches get `Agent::Omp | Agent::Copilot => unreachable!("owned integrations return above")`. Prefer making `claude_status`/`codex_status` selection a separate `fn merged_status(agent, text)` that only Claude/Codex reach, so no `unreachable!` is needed; either way no `unwrap`. New functions:

```rust
/// omp and Copilot load every file in one directory, so Mkrew owns one file there.
fn owned_contents(agent: Agent) -> Option<&'static str> {
    match agent {
        Agent::Omp => Some(OMP_EXTENSION),
        Agent::Copilot => Some(COPILOT_HOOKS),
        Agent::Claude | Agent::Codex => None,
    }
}

/// Another text = an older Mkrew version or the user's edit: install replaces it.
fn owned_status(text: Option<&str>, ours: &str) -> Installed {
    match text {
        None => Installed::No,
        Some(text) if text == ours => Installed::Yes,
        Some(_) => Installed::Partly,
    }
}

/// Install writes the whole file, uninstall deletes it; a different file is backed up first.
fn run_owned(
    action: Action,
    name: &str,
    file: &Path,
    text: Option<&str>,
    ours: &str,
) -> anyhow::Result<ExitCode> {
    let installed = owned_status(text, ours);
    let backup = match (action, &installed) {
        (Action::Status, _) => return Ok(print_status(name, file, &installed)),
        (Action::Install, Installed::Yes) | (Action::Uninstall, Installed::No) => {
            let verb = if action == Action::Install { "installed" } else { "uninstalled" };
            println!("{name}: already {verb} ({})", file.display());
            return Ok(ExitCode::SUCCESS);
        }
        (_, Installed::Partly) => Some(back_up(file)?),
        _ => None,
    };
    if action == Action::Install {
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        fs::write(file, ours).with_context(|| format!("write {}", file.display()))?;
    } else {
        fs::remove_file(file).with_context(|| format!("delete {}", file.display()))?;
    }
    let verb = if action == Action::Install { "installed" } else { "uninstalled" };
    match backup {
        Some(backup) => println!("{name}: {verb} ({}; backup: {})", file.display(), backup.display()),
        None => println!("{name}: {verb} ({})", file.display()),
    }
    Ok(ExitCode::SUCCESS)
}

/// The shipped `resume` line of `agent`'s profile (SPEC §6).
fn resume_example(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude --resume {session_id}",
        Agent::Codex => "codex resume {session_id}",
        Agent::Omp => "omp --resume {session_id}",
        Agent::Copilot => "copilot --resume={session_id}",
    }
}

/// A note when the user's profile cannot resume: seeding never overwrites an existing
/// profile, so one written before `resume` shipped stays without it. `None` when the profile
/// resumes, is missing (seeded on the next daemon start) or does not parse (the UI says so).
fn resume_hint(agent: Agent, file: &Path, text: Option<&str>) -> Option<String> {
    let profile = mkrew_core::profile::parse_profile(name(agent), file, text?).ok()?;
    profile.resume.is_none().then(|| {
        format!(
            "note: {} has no `resume`; add `resume = \"{}\"` so its conversations come back after a restart",
            file.display(),
            resume_example(agent)
        )
    })
}
```

In `run`, after `run_on` succeeds for `Action::Install`, print the hint for the user's profile:

```rust
pub fn run(action: Action, agent: Agent) -> anyhow::Result<ExitCode> {
    let file = config_file(agent, |key| {
        std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
    })?;
    let code = run_on(action, agent, &file)?;
    if action == Action::Install {
        let profile = mkrew_core::Paths::from_env()?.agents_dir().join(format!("{}.toml", name(agent)));
        let text = fs::read_to_string(&profile).ok();
        if let Some(hint) = resume_hint(agent, &profile, text.as_deref()) {
            println!("{hint}");
        }
    }
    Ok(code)
}
```

Update the `Install` doc line in `crates/mkrew-cli/src/main.rs:164` to `/// Add Mkrew's hooks to the agent's config (backs the file up first; omp/copilot: one Mkrew-owned file).`

- [ ] **Step 5: Run the tests**

Run: `cargo test -p mkrew-cli integration::`
Expected: PASS.

- [ ] **Step 6: Smoke** (temp dirs, never the real config):

```bash
PI_CODING_AGENT_DIR=$(mktemp -d) COPILOT_HOME=$(mktemp -d) bash -c 'for a in omp copilot; do cargo run -q -p mkrew-cli -- integration install $a; cargo run -q -p mkrew-cli -- integration status $a; cargo run -q -p mkrew-cli -- integration uninstall $a; done'
```

Expected: `installed`, `installed (…)`, `uninstalled` for both. On this machine the omp install also prints the `note: …omp.toml has no resume…` line.

- [ ] **Step 7: Suggested message:** `feat(cli): integration install for omp and copilot`

---

### Task 4: Store — remember sessions the daemon stop interrupted

**Files:**
- Create: `crates/mkrew-store/migrations/0010_session_interrupted.sql`
- Modify: `crates/mkrew-store/src/lib.rs:17-27,641-650`
- Test: `crates/mkrew-store/src/tests.rs`

**Interfaces:**
- Produces: `Store::mark_interrupted(&self, id: Ulid) -> Result<()>` and `Store::take_interrupted_sessions(&self) -> Result<Vec<Ulid>>`. `mark_stale_sessions_exited` also sets `interrupted = 1`.

- [ ] **Step 1: Write the failing test** in `tests.rs`:

```rust
#[test]
fn interrupted_sessions_are_taken_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let live = Session { state: SessionState::Working, ..session(p.id, 1) };
    let ended = Session {
        state: SessionState::Exited,
        exit_code: Some(0),
        ended_at: Some(5),
        ..session(p.id, 2)
    };
    let stopped = Session { state: SessionState::Exited, ended_at: Some(6), ..session(p.id, 3) };
    for s in [&live, &ended, &stopped] {
        store.insert_session(s).unwrap();
    }
    store.mark_interrupted(stopped.id).unwrap();
    // A reboot left `live` running: stale rows are interrupted too.
    assert_eq!(store.mark_stale_sessions_exited().unwrap(), 1);

    let mut taken = store.take_interrupted_sessions().unwrap();
    taken.sort();
    let mut expected = vec![live.id, stopped.id];
    expected.sort();
    assert_eq!(taken, expected, "the session that ended by itself is not resumed");
    assert!(store.take_interrupted_sessions().unwrap().is_empty());
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-store interrupted_sessions_are_taken_once`
Expected: FAIL to compile (no `mark_interrupted`).

- [ ] **Step 3: Implement.** `0010_session_interrupted.sql`:

```sql
-- Sessions the daemon stop (or a reboot) cut off; resumed once at the next start (SPEC §8).
ALTER TABLE session ADD COLUMN interrupted INTEGER NOT NULL DEFAULT 0;
```

Add `include_str!("../migrations/0010_session_interrupted.sql"),` to `MIGRATIONS`. Replace `mark_stale_sessions_exited` and add the two methods:

```rust
    /// Marks every session not yet `exited` as exited and interrupted: no PTY survives a
    /// daemon restart, and a row still live here means a crash or reboot (SPEC §8).
    /// Returns how many rows changed.
    pub fn mark_stale_sessions_exited(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE session SET state = 'exited', ended_at = COALESCE(ended_at, ?1),
                state_source = 'daemon', state_changed_at = ?1, interrupted = 1
             WHERE state <> 'exited'",
            [now_ms()],
        )?)
    }

    /// The daemon stopped while `id` ran: resumable at the next start (SPEC §8).
    pub fn mark_interrupted(&self, id: Ulid) -> Result<()> {
        self.conn
            .execute("UPDATE session SET interrupted = 1 WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    /// The sessions the last daemon stop interrupted. Clears the mark, so each one is
    /// resumed at most once.
    pub fn take_interrupted_sessions(&self) -> Result<Vec<Ulid>> {
        let mut stmt = self
            .conn
            .prepare("UPDATE session SET interrupted = 0 WHERE interrupted = 1 RETURNING id")?;
        let ids = stmt.query_map([], |row| row.get::<_, String>(0))?;
        ids.map(|id| parse_ulid("session.id", id?)).collect()
    }
```

- [ ] **Step 4: Run the store tests**

Run: `cargo test -p mkrew-store`
Expected: PASS (`stale_sessions_are_marked_exited` is unchanged: it does not read the new column).

- [ ] **Step 5: Suggested message:** `feat(store): remember sessions a daemon stop interrupted`

---

### Task 5: Daemon — resume free agent panes in their tab

**Files:**
- Modify: `crates/mkrew-daemon/src/session.rs:1284-1317` (`Actor::finish`)
- Modify: `crates/mkrew-daemon/src/server/resume.rs:1-124`
- Modify: `crates/mkrew-daemon/src/server.rs:97-120`
- Test: `crates/mkrew-daemon/tests/daemon.rs` (after `restart_resumes_running_tasks_with_their_native_session_id`)

**Interfaces:**
- Consumes: Task 4 `mark_interrupted`, `take_interrupted_sessions`.
- Produces: `Daemon::resume_sessions(&self)` (replaces `resume_tasks`), and `Daemon::resume(&self, last: &Session, task: Option<&Task>) -> Option<Ulid>`.

- [ ] **Step 1: Write the failing end-to-end test** in `daemon.rs`:

```rust
/// SPEC §8: an agent pane without a task that a tab showed when the daemon stopped comes back
/// in the same pane with its conversation. A session in no tab stays exited, and each stop
/// resumes the current pane, never an older one.
#[test]
fn restart_resumes_free_agent_panes_in_tabs_once() {
    let env = Env::new();
    let scenario = env.dir.path().join("native.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-7\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let fake = built_bin("mkrew-fakeagent");
    let args = format!("args = ['--scenario', '{}']", scenario.display());
    env.write_fake_profile_as("resumable", &args["args = ".len()..]);
    let path = env.dir.path().join("config/agents/resumable.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let resume = format!("resume = '\"{}\" --resume {{session_id}}'\n", fake.display());
    std::fs::write(&path, text.replace("[state]", &format!("{resume}\n[state]"))).unwrap();

    let list = |client: &mut Client| -> Vec<Session> {
        client
            .call(Request::SessionList(SessionListParams::default()))
            .into_result()
            .unwrap()
    };
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let shown: SessionSpawnResult = client.spawn(&project, "resumable").into_result().unwrap();
    let hidden: SessionSpawnResult = client.spawn(&project, "resumable").into_result().unwrap();
    let tab: TerminalTab = client
        .call(create_tab(&project, "agents"))
        .into_result()
        .unwrap();
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: json!({"session": shown.session_id.to_string()}),
        }))
        .into_result::<()>()
        .unwrap();
    for id in [shown.session_id, hidden.session_id] {
        let deadline = Instant::now() + TIMEOUT;
        while client.session(id).native_session_id.is_none() {
            assert!(Instant::now() < deadline, "no native session id reported");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    client.stop();
    assert!(daemon.wait_exit().success());

    let (mut daemon, mut client) = env.start();
    let sessions = list(&mut client);
    assert_eq!(sessions.len(), 3, "only the shown pane came back: {sessions:?}");
    let resumed = sessions
        .iter()
        .find(|s| s.id != shown.session_id && s.id != hidden.session_id)
        .unwrap()
        .clone();
    assert_eq!(
        (resumed.task_id, resumed.agent_profile.as_str(), resumed.native_session_id.as_deref()),
        (None, "resumable", Some("native-7"))
    );
    client.wait_state(resumed.id, SessionState::NeedsInput);
    let attached: SessionAttachResult = client.attach(resumed.id).into_result().unwrap();
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains("Resumed: native-7"), "{screen}");
    assert_eq!(
        list_tabs(&mut client, &project)[0].layout,
        json!({"session": resumed.id.to_string()})
    );

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let sessions = list(&mut client);
    assert_eq!(sessions.len(), 4, "one more pane, resumed from the current one: {sessions:?}");
    let newest = sessions.iter().max_by_key(|s| (s.started_at, s.id)).unwrap();
    assert_eq!(
        list_tabs(&mut client, &project)[0].layout,
        json!({"session": newest.id.to_string()})
    );
    client.stop();
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-daemon --test daemon restart_resumes_free_agent_panes_in_tabs_once`
Expected: FAIL (`only the shown pane came back`: 2 sessions, nothing resumed).

- [ ] **Step 3: Mark shutdown exits.** In `Actor::finish`, right before `self.session.exit_code = exit_code;`:

```rust
        // Cut off by the daemon stop, not ended by the user or the agent: resume it at the
        // next start (SPEC §8). A reboot leaves the row live instead; startup marks those.
        if self.hub.stopping.load(Ordering::SeqCst)
            && let Err(e) = self.hub.store().mark_interrupted(self.session.id)
        {
            tracing::warn!(id = %self.session.id, "not marked interrupted: {e}");
        }
```

(`stopping` is a private field of `Hub` in the same module. `Ordering` is already imported there.)

- [ ] **Step 4: Resume free panes.** In `resume.rs` replace the module doc, `resume_tasks` and the head of `resume`:

```rust
//! Startup resume (SPEC §8): tasks the last daemon left running/needs_input, and agent panes
//! without a task that a tab still showed when the daemon stopped, get their agent
//! conversation back. A task that cannot resume is flagged for the user to run again. A
//! fan-out's original task is left alone.
```

```rust
    /// For each running/needs_input task: relaunch its last session with the profile's
    /// `resume` command when the agent reported its own id, else move the card to needs_input
    /// with `error_flag` ("session stopped — run again?"). Then the same for every interrupted
    /// agent session without a task that is still in a tab; one that cannot resume stays
    /// exited, as before.
    pub(super) async fn resume_sessions(&self) {
        let found = {
            let store = self.store();
            store.take_interrupted_sessions().and_then(|interrupted| {
                Ok((store.list_tasks(None, None)?, store.list_sessions(None)?, interrupted))
            })
        };
        let (tasks, sessions, interrupted) = match found {
            Ok(found) => found,
            Err(e) => {
                tracing::error!("cannot resume sessions: {e}");
                return;
            }
        };
        // A fan-out's original runs nothing itself: its children are resumed (SPEC §20E.2).
        let fanned_out: Vec<Ulid> = tasks.iter().filter_map(|t| t.fanout_of).collect();
        let stopped_tasks = tasks.iter().filter(|t| {
            matches!(t.status, TaskStatus::Running | TaskStatus::NeedsInput)
                && !fanned_out.contains(&t.id)
        });
        for task in stopped_tasks {
            let resumed = match last_session(&sessions, task.id) {
                Some(last) => self.resume(last, Some(task)).await,
                None => None,
            };
            if resumed.is_none() {
                self.mark_stopped(task.id);
            }
        }
        let panes = sessions.iter().filter(|s| {
            s.task_id.is_none() && s.kind == SessionKind::Agent && interrupted.contains(&s.id)
        });
        for last in panes {
            if self.in_a_tab(last) && self.resume(last, None).await.is_none() {
                tracing::info!(session_id = %last.id, "pane not resumed: no agent session id or no `resume`");
            }
        }
    }

    /// Spawns `last`'s resume command as a new session (of `task`, if any) in `last`'s pane slot.
    async fn resume(&self, last: &Session, task: Option<&Task>) -> Option<Ulid> {
        let native = last.native_session_id.as_deref()?;
        let profile = match self.load_profile(&last.agent_profile) {
            Ok(profile) => profile.resumed(native)?,
            Err(outcome) => {
                tracing::warn!(session_id = %last.id, "cannot resume: {outcome:?}");
                return None;
            }
        };
        let project = match self.store().get_project(last.project_id) {
            Ok(project) => project,
            Err(e) => {
                tracing::warn!(session_id = %last.id, "cannot resume: {e}");
                return None;
            }
        };
        let project_id = project.id;
        // A worktree task resumes in its worktree (SPEC §20E.1), never in the project folder.
        let dir = match task.and_then(|t| t.worktree_path.as_ref()) {
```

Keep the rest of the worktree block, but replace `task_id = %task.id` with `session_id = %last.id` in its `warn!`. In `Start` use `task_id: task.map(|t| t.id),`. In the success log use `tracing::info!(session_id = %last.id, %new_id, native, "session resumed");`, renaming the matched `session_id` binding to `new_id`. In the failure log use `session_id = %last.id`. Add after `replace_in_tabs`:

```rust
    /// Whether a tab of `session`'s project still shows it (the user did not close or hide it).
    fn in_a_tab(&self, session: &Session) -> bool {
        let tabs = self.store().list_tabs(session.project_id);
        match tabs {
            Ok(tabs) => tabs
                .iter()
                .any(|tab| replace_session(&tab.layout, session.id, session.id).is_some()),
            Err(e) => {
                tracing::warn!(session_id = %session.id, "tabs not read, pane not resumed: {e}");
                false
            }
        }
    }
```

Imports: `use mkrew_protocol::{Event, Session, SessionKind, Task, TaskStatus};`. In the test module drop the now-duplicate `SessionKind` from its own `use mkrew_protocol::{…}`.

In `server.rs:120` call `self.resume_sessions().await;`. In the `serve` doc (lines 97-101) write "resumes interrupted tasks and agent panes".

- [ ] **Step 5: Run the daemon tests**

Run: `cargo test -p mkrew-daemon --test daemon restart_resumes ; cargo test -p mkrew-daemon --test daemon worktree_tasks_run_and_resume_in_their_own_worktree ; cargo test -p mkrew-daemon --test daemon daemon_stop_terminates_live_sessions ; cargo test -p mkrew-daemon --lib resume`
Expected: PASS.

- [ ] **Step 6: Suggested message:** `feat(daemon): resume free agent panes after a restart`

---

### Task 6: Spec, ADR, manual checks, full verify

**Files:**
- Create: `docs/decisions/0017-resume-free-panes-omp-copilot.md`
- Modify: `docs/SPEC.md` §3 (lines 135-137), §5 (line 298), §6 (after the omp profile block, line 379; hook integrations lines 408-422), §8 (lines 546-548), §9 (line 579)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify"), `docs/PROGRESS.md` ("Notes for the next session")

- [ ] **Step 1: ADR 0017** (follow `docs/decisions/0000-template.md`). Context: the user works mostly with omp, Claude, Codex and Copilot and wants herdr-like persistence. Before this change only task sessions of Claude/Codex resumed. Options: (1) hook agents + owned integration files + resume of interrupted free panes still in a tab (chosen); (2) screen-scrape agent session ids (fragile, per-agent regexes); (3) `--continue` without an id (resumes the newest conversation of the folder, wrong with two panes in one project). Consequences: two more hook agents in `mkrew_core::hook`, migration 0010, user profiles written before this change need the `resume` line by hand (install prints it).

- [ ] **Step 2: SPEC.**
  - §3, replace the reboot bullet: "`mk daemon stop` or a reboot → agents stop. On next start the daemon restores workspaces, tasks, tabs and sessions; agents whose profile has `resume` and that reported their own session id are relaunched into their old conversation: task sessions (running/needs_input), and agent panes without a task that a tab still showed."
  - §5, line 298: "v2 migrations 0008 (M11) and 0009 (M13) are listed in §20; `0010_session_interrupted.sql` adds `session.interrupted INTEGER NOT NULL DEFAULT 0` (ADR 0017)."
  - §6: change the omp profile block to the Task 2 `agents/omp.toml`, then add the `agents/copilot.toml` block. In "Hook integrations" add: "- Oh My Pi: `mk integration install omp` writes the Mkrew-owned extension `<PI_CODING_AGENT_DIR or ~/.omp/agent>/extensions/mkrew.ts`; it runs `mk hook omp <busy|needs_input|done|session_start> {"session_id": …}` (root session only: not with `OMPCODE=1`, not without a UI). - GitHub Copilot CLI: `mk integration install copilot` writes the Mkrew-owned `<COPILOT_HOME or ~/.copilot>/hooks/mkrew.json` (`sessionStart`, `userPromptSubmitted`, `agentStop`, `notification` matching `permission_prompt|elicitation_dialog`), each `exec: mk hook copilot <event>`, JSON on stdin, `sessionId` = `native_session_id`. - Owned files: install writes the whole file (backing up a different one), uninstall deletes it. Install prints the `resume` line when the user's profile lacks one." Extend the event mapping sentence: "omp `busy`/`needs_input`/`done` like fake; Copilot `userPromptSubmitted` → working, `notification` `permission_prompt`/`elicitation_dialog` → needs_input, `agentStop` → done."
  - §8 Resume bullet, append: "Then, for each session marked `interrupted` (it exited during the daemon stop, or was still live at startup after a crash/reboot) that has no task, is an agent, and is still a leaf of a tab layout: spawn the resume command in the same pane. One that cannot resume stays exited. The mark is cleared at startup, so each session is resumed at most once."
  - §9: `mk integration install|uninstall|status <claude|codex|omp|copilot>`.

- [ ] **Step 3: HUMAN_CHECKS "To verify"** — add a section "2026-10-07 resume omp/Copilot/free panes":
  1. Add `resume = "omp --resume {session_id}"` to `%APPDATA%\mkrew\agents\omp.toml` and set `mode = "hook+screen"`. Run `mk integration install omp` and `mk integration install copilot`.
  2. Open a project, `+ Agent` with omp, chat one turn; same with copilot, claude, codex (no task). Close the UI, reopen it: all panes are still live (unchanged behaviour).
  3. `mk daemon stop`, then reopen the UI: each pane comes back in the same tab position with the same conversation (`omp --resume`, `copilot --resume=`, …).
  4. Reboot Windows, open the UI: same as 3.
  5. In omp, ask it to run `omp -p hi` in its bash tool; after the restart the pane still resumes the original conversation (nested guard).
  6. Close an agent pane (kill), restart: it does not come back.
  7. In omp/copilot: a tool approval shows `needs_input` (`?`), and finishing a turn shows `done`.

- [ ] **Step 4: PROGRESS note** under "Notes for the next session": "- 2026-10-07 resume plan (docs/superpowers/plans/2026-10-07-resume-omp-copilot-panes.md): hook agents omp/copilot, `mk integration install omp|copilot` (owned files), migration 0010 `session.interrupted`, free agent panes in a tab resume after a restart, ADR 0017; manual checks in HUMAN_CHECKS. Outside the milestone loop."

- [ ] **Step 5: Full verify**

Run: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh` (fallback: fmt check, clippy `-D warnings`, `cargo test --workspace -j 4`).
Expected: all green.

- [ ] **Step 6: Suggested message:** `docs: resume omp, copilot and free agent panes (ADR 0017)`
