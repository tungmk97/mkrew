# Hooks reach the daemon with its own `mk` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A hook started inside a Mkrew pane always reports through an `mk` of the same version as the pane's daemon, whatever `PATH` says, and a failed `mk hook` leaves a log line.

**Architecture:** Three parts, together the smallest design that covers all four agents:
1. **Daemon → PTY env.** The daemon sets `MKREW_MK` = absolute path of the `mk` next to its own exe (the existing `mk_exe()` lookup of `mk web`, moved to `spawn.rs`) in every PTY, like `MKREW_SESSION_ID`/`MKREW_SOCKET`.
2. **Hook configs never run a bare `mk`.** omp's extension (JavaScript) runs `process.env.MKREW_MK || "mk"`. Claude Code, Codex and Copilot configs cannot read an env var portably (table below), so `mk integration install` writes the absolute path of the `mk` that runs the install (`current_exe`). Old bare-`mk` entries are recognised as Mkrew's and replaced.
3. **`mk hook` forwards to `MKREW_MK`.** When `MKREW_MK` names another existing file than the running `mk`, `mk hook` re-runs itself through that `mk` (same args, inherited stdio) and returns its exit code. So even the absolute path of another install's `mk` ends in the daemon's own `mk`. A failure of `mk hook` (no daemon, `version_mismatch`, timeout, bad input) appends one line to `<data>/logs/hook.log`. The daemon also logs a `warn` line when it refuses a `hello`.

No protocol change: `PROTOCOL_VERSION` stays 41 (env var + CLI only). Users reinstall integrations once (`mk integration status` reports `partly installed` for an old one).

**Tech Stack:** Rust (clap, serde_json, toml_edit, dunce, interprocess), TypeScript asset for omp.

**Spec:** the prompt below (2026-10-14 chat, "Prompt 2"), plus SPEC §6 (`docs/SPEC.md:356`, env + hooks bullets ~464-485) and §7 (`hook.report`, ~538). This plan changes SPEC §6 and adds ADR 0055.

<details><summary>Spec (verbatim)</summary>

```text
Fix: agent hooks call whatever `mk` is first on PATH; a stale mk with another protocol version fails
silently, so `native_session_id` is never stored and nothing resumes after a daemon restart.

Evidence (2026-10-14, user's machine)
- crates/mkrew-cli/assets/omp-extension.ts line 23: `execFile("mk", ["hook","omp",…], …, () => done())`
  ignores every error. Claude/Codex hooks also call bare `mk` (integration.rs `CODEX_NOTIFY`, Claude hook
  command strings).
- The user PATH had mk 0.1.0 first; the daemon was 0.1.5 (PROTOCOL_VERSION 41). server.rs `hello`
  (~line 459) rejects other versions with version_mismatch. Result: since 2026-10-12 17:44 no omp
  session got a native_session_id (65 of 132 omp sessions have one, none after that time). On restart
  the daemon logged `pane not resumed: no agent session id or no resume` for all 10 omp panes.

Goal
- A hook started inside a Mkrew pane always reaches the daemon with an `mk` of the same version as that
  daemon, whatever PATH says. Suggested (verify it fits SPEC 6 env rules): the daemon puts the absolute
  path of the `mk` next to its own exe into the PTY env (like MKREW_SESSION_ID / MKREW_SOCKET), and the
  omp extension, Claude hooks, Codex notify and Copilot hooks use it, falling back to `mk`. Pick the
  smallest design that covers all four agents; if a hook config format cannot use an env var, say so
  and handle that agent the best way its docs allow (look up the CURRENT docs, do not guess).
- A failed `mk hook` must leave a trace: version mismatch and other failures go to a log line the user
  can find (`mk hook` exits silently today). Hooks must still never block or break the agent.

Constraints
- Spec change → update SPEC 6/7 and add an ADR in docs/decisions (check the latest number first).
- If PROTOCOL_VERSION must change, bump it and say why. Reinstalling integrations
  (`mk integration install <agent>`) may be needed; print or document that.
- Follow AGENTS.md: tests, no real agent or network in tests, `sh scripts/verify.sh`, Conventional
  Commit, commit only your own hunks.

Acceptance
- Test: the PTY env of a spawned session contains the mk path, and the omp extension asset uses it.
- Test: `mk hook` against a daemon with another protocol version leaves the log line.
- Manual check for HUMAN_CHECKS.md: start an omp pane, `mk session list` / DB shows a
  native_session_id; `mk daemon stop` then reopen Mkrew → the pane resumes.
```
</details>

## Agent config formats (current docs, read 2026-10-14)

| Agent | Can the config read `MKREW_MK`? | Choice |
|---|---|---|
| omp | Yes: the extension is JavaScript (`process.env`). | `execFile(process.env.MKREW_MK \|\| "mk", …)` |
| Claude Code | Only in **shell form**, and that shell is Git Bash on Windows, **PowerShell when Git Bash is missing** (<https://code.claude.com/docs/en/hooks>, "Exec form and shell form"). One string cannot expand a variable in both. **Exec form** (`command` + `args`) runs the executable directly: no shell, no expansion. | Exec form: `{"type": "command", "command": "<abs mk>", "args": ["hook", "claude", "<Event>"]}` |
| Codex | No: `notify` is an argv array run without a shell. | `notify = ["<abs mk>", "hook", "codex"]` |
| Copilot CLI | `exec` + `args` run without a shell; the `bash`/`powershell` fields would need two scripts plus PowerShell stdin forwarding (<https://docs.github.com/en/copilot/reference/hooks-reference>). | Keep `exec`, value `<abs mk>` |

`<abs mk>` = `dunce::canonicalize(std::env::current_exe())` of the `mk` running `mk integration install`. When it is not the daemon's (two installs), part 3 (forwarding) still reaches the daemon's `mk`.

## Global Constraints

- `PROTOCOL_VERSION` stays `41`. No migration.
- ADR number: latest is `0054` → this plan uses **0055**. Re-check `docs/decisions/` first; if 0055 exists (e.g. the data-dir retention plan landed first), take the next free number and use it everywhere below.
- Env var: `MKREW_MK` (const `mkrew_core::paths::ENV_MK`). The daemon's test override stays `MKREW_MK_EXE` (`mkrew_daemon::ENV_MK_EXE`).
- Log file: `<data_dir>/logs/hook.log` (`Paths::logs_dir()`), one line per failure, rotated to `hook.log.1` above 1 MiB.
- Hooks never block or break the agent: `mk hook` keeps its 5 s `REPORT_TIMEOUT`; logging ignores its own I/O errors; forwarding inherits stdio and returns the child's exit code.
- No test calls a real agent or the network. Tests that spawn `mk` must `env_remove("MKREW_MK")` (cargo may run inside a Mkrew pane).
- No `unwrap`/`expect` outside tests. `anyhow` in binaries.
- Shell: in omp `bash` may be WSL; run `cargo`/`git` directly, `sh scripts/verify.sh` (Git Bash). Build with `-j 4`. Rebuild before CLI integration tests: `cargo build -j 4 -p mkrew-cli -p mkrew-daemon -p mkrew-fakeagent`.
- Never touch the user's real agent configs (`~/.claude`, `~/.codex`, `~/.omp`, `~/.copilot`) or the running daemon. Smoke with `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `PI_CODING_AGENT_DIR`, `COPILOT_HOME`, `MKREW_DATA_DIR`, `MKREW_SOCKET` pointing at temp dirs.
- Conventional Commits; explicit paths, `git add -p` for files with others' WIP; never `git add -A`, never push.

## Review Focus

1. **`MKREW_MK` is the running `mk` itself** (normal case after reinstall) → no forwarding loop: canonical paths compared. Pinned by `hook_forwards_to_the_daemons_mk_and_logs_failures` (Task 2): the copy is reached once and does not forward again (exactly one log line, naming the copy).
2. **`MKREW_MK` names a missing file or is the bare fallback `mk`** (daemon found no sibling `mk`) → no forwarding; the running `mk` reports itself. Pinned by `forward_target_ignores_relative_missing_and_itself` (Task 2).
3. **Old installs** (`mk hook claude Stop` string, `notify = ["mk","hook","codex"]`, Copilot `"exec": "mk"`) → status `partly installed`, install replaces them in place, uninstall removes old and new forms, user hooks stay. Pinned by `claude_install_replaces_an_old_bare_mk_hook`, `codex_install_upgrades_an_older_mkrew_notify`, `owned_files_install_replace_and_uninstall` (Task 3).
4. **`mk` path with spaces/backslashes** (`C:\Program Files\…`, `C:\Users\Nguyễn Tùng\…`) → one argument in exec forms; escaping by serde_json/toml_edit. Pinned by the `MK` test constant (Task 3), which contains a space and backslashes.
5. **Logging itself fails** (logs dir missing, read-only) → the hook still exits with its original error, no panic. `log_failure` creates the dir and ignores every I/O error; pinned by `hook_log_rotates_above_the_cap` in a fresh temp dir (Task 2).

---

### Task 1: Daemon puts `MKREW_MK` into every PTY

**Files:**
- Modify: `crates/mkrew-core/src/paths.rs` (new `ENV_MK` next to `ENV_SOCKET`, line ~13)
- Modify: `crates/mkrew-daemon/src/spawn.rs` (move `ENV_MK_EXE` + `mk_exe()` here from `web.rs`)
- Modify: `crates/mkrew-daemon/src/web.rs:25,240-245` (call `spawn::mk_exe`, delete the moved items)
- Modify: `crates/mkrew-daemon/src/lib.rs:40` (`pub use spawn::ENV_MK_EXE;`)
- Modify: `crates/mkrew-daemon/src/session.rs:379-383` (set `MKREW_MK`)
- Modify: `crates/mkrew-daemon/src/server.rs:458-467` (`hello`: warn on mismatch)
- Test: `crates/mkrew-daemon/tests/daemon/states.rs`

**Interfaces:**
- Produces: `pub const ENV_MK: &str = "MKREW_MK";` in `mkrew_core::paths`; `pub fn mk_exe() -> std::path::PathBuf` and `pub const ENV_MK_EXE: &str = "MKREW_MK_EXE";` in `mkrew_daemon::spawn` (still re-exported as `mkrew_daemon::ENV_MK_EXE`).

- [ ] **Step 1: Write the failing test** in `crates/mkrew-daemon/tests/daemon/states.rs`, next to `session_env_cwd_and_exit_code_reach_the_store` (same helpers):

```rust
/// Hooks inside the pane run the daemon's own `mk` (SPEC §6, ADR 0055).
#[test]
fn session_env_names_the_daemons_mk() {
    let env = Env::new();
    let script = if cfg!(windows) {
        "command = 'cmd.exe'\nargs = ['/c', 'echo', 'mk=[%MKREW_MK%]']"
    } else {
        "command = 'sh'\nargs = ['-c', 'echo \"mk=[$MKREW_MK]\"']"
    };
    env.write_profile("probe", &format!("name = \"Probe\"\n{script}\n"));
    let mk = env.data_dir().join("bin").join("mk.exe");
    let (_daemon, mut client) = env.start_with(&[(mkrew_daemon::ENV_MK_EXE, mk.as_path())]);
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "probe").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::Exited);
    let log = session_log(&env, spawned.session_id);
    assert!(log.contains(&format!("mk=[{}]", mk.display())), "{log}");
    client.stop();
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -j 4 -p mkrew-daemon --test daemon session_env_names_the_daemons_mk`
Expected: FAIL — the log has `mk=[%MKREW_MK%]` (Windows) or `mk=[]`.

- [ ] **Step 3: Add the const** in `crates/mkrew-core/src/paths.rs` below `ENV_SOCKET`:

```rust
/// The `mk` next to the daemon. Set inside every session PTY (SPEC §6): agent hooks run this
/// `mk`, so they speak the daemon's protocol whatever `PATH` finds first.
pub const ENV_MK: &str = "MKREW_MK";
```

- [ ] **Step 4: Move the lookup** from `web.rs` to `crates/mkrew-daemon/src/spawn.rs` (delete it from `web.rs`; `web::start` passes the function itself, so line ~63 becomes `tokio::spawn(supervise(web, settings, spawn::mk_exe))`; `lib.rs:40` becomes `pub use spawn::ENV_MK_EXE;`):

```rust
/// Names the program to use instead of looking for `mk` (tests).
pub const ENV_MK_EXE: &str = "MKREW_MK_EXE";

/// `MKREW_MK_EXE` when set (tests), else `mk` next to the running daemon, else `mk` for `PATH`.
pub fn mk_exe() -> PathBuf {
    match std::env::var_os(ENV_MK_EXE).filter(|v| !v.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => sibling_exe("mk").unwrap_or_else(|_| PathBuf::from("mk")),
    }
}
```

- [ ] **Step 5: Set it in the PTY** — `crates/mkrew-daemon/src/session.rs`, after `cmd.env(mkrew_core::paths::ENV_SOCKET, &socket);`:

```rust
        cmd.env(mkrew_core::paths::ENV_MK, crate::spawn::mk_exe());
```

(If `cmd.env` wants another type, pass `crate::spawn::mk_exe().into_os_string()`.)

- [ ] **Step 6: Log a refused hello** — `crates/mkrew-daemon/src/server.rs` `hello`, right after `let message = format!(…);`:

```rust
            tracing::warn!(client = ?params.client, "hello refused: {message}");
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -j 4 -p mkrew-daemon --test daemon session_env && cargo test -j 4 -p mkrew-daemon --lib spawn && cargo test -j 4 -p mkrew-daemon --test daemon web`
Expected: all pass (`mk web` tests still find their program through `ENV_MK_EXE`).

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-core/src/paths.rs crates/mkrew-daemon/src/spawn.rs crates/mkrew-daemon/src/web.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/session.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/tests/daemon/states.rs
git commit -m "feat(daemon): set MKREW_MK to the daemon's own mk in every PTY"
```

### Task 2: `mk hook` forwards to `MKREW_MK` and logs failures

**Files:**
- Modify: `crates/mkrew-cli/src/hook.rs`
- Test: `crates/mkrew-cli/src/hook.rs` (`mod tests`), `crates/mkrew-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `mkrew_core::paths::ENV_MK` (Task 1); `Paths::from_env()`, `Paths::logs_dir()`.
- Produces: `<logs_dir>/hook.log` lines `"<unix ms> <canonical exe path> mk <version> hook <agent>: <error>"`; `fn forward_target(mk: Option<OsString>, me: &Path) -> Option<PathBuf>`; `fn log_failure(logs: &Path, agent: &str, error: &anyhow::Error)`; `const HOOK_LOG_CAP: u64`.

- [ ] **Step 1: Write the failing CLI tests** in `crates/mkrew-cli/tests/cli.rs`. First add `.env_remove("MKREW_MK")` to `Env::command`. Then:

```rust
/// A daemon of another protocol version: answers the first request (`hello`) with
/// `version_mismatch`, like `server.rs` `hello` does.
fn daemon_of_another_protocol(socket: &str) -> std::thread::JoinHandle<()> {
    use std::io::{Read, Write};
    use interprocess::local_socket::{ListenerOptions, traits::Listener as _};
    use mkrew_protocol::{ErrorCode, Frame, FrameDecoder, Message, Outcome, encode};
    let listener = ListenerOptions::new()
        .name(mkrew_daemon::ipc::name(socket).unwrap())
        .create_sync()
        .unwrap();
    std::thread::spawn(move || {
        let mut conn = listener.accept().unwrap();
        let mut decoder = FrameDecoder::default();
        let mut buf = [0u8; 4096];
        let id = loop {
            let n = conn.read(&mut buf).unwrap();
            decoder.push(&buf[..n]);
            if let Some(Frame::Message(Message::Request { id, .. })) = decoder.next_frame().unwrap() {
                break id;
            }
        };
        let message = "client speaks protocol 41, daemon speaks 40; restart the daemon";
        let reply = Message::Response { id, outcome: Outcome::error(ErrorCode::VersionMismatch, message) };
        conn.write_all(&encode(&Frame::Message(reply)).unwrap()).unwrap();
    })
}

fn hook_log(env: &Env) -> String {
    std::fs::read_to_string(env.dir.path().join("data").join("logs").join("hook.log"))
        .unwrap_or_default()
}

/// A hook that meets a daemon of another version fails (exit 1, never blocks) and says why
/// in `<data>/logs/hook.log`.
#[test]
fn hook_against_another_protocol_leaves_a_log_line() {
    let env = Env::new();
    let daemon = daemon_of_another_protocol(&env.socket);
    let output = env
        .command(&["hook", "omp", "busy", "{}"])
        .env("MKREW_SESSION_ID", ulid::Ulid::new().to_string())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    daemon.join().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let log = hook_log(&env);
    assert!(log.contains("hook omp: client speaks protocol 41, daemon speaks 40"), "{log}");
}

/// Inside a pane, `mk hook` runs the daemon's `mk` (`MKREW_MK`) when that is another file.
#[test]
fn hook_forwards_to_the_daemons_mk_and_logs_failures() {
    let env = Env::new();
    let copy = env.dir.path().join("other bin").join(format!("mk{}", std::env::consts::EXE_SUFFIX));
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_mk"), &copy).unwrap();
    // No daemon runs: the report fails, and the one line names the `mk` that tried.
    let output = env
        .command(&["hook", "omp", "busy", "{}"])
        .env("MKREW_SESSION_ID", ulid::Ulid::new().to_string())
        .env("MKREW_MK", &copy)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let log = hook_log(&env);
    let copy = dunce::canonicalize(&copy).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains(&copy.display().to_string()), "{log}");
}
```

(`env.dir` / `env.socket` are the test `Env`'s fields. `dunce`, `ulid`, `interprocess`, `mkrew_protocol`, `mkrew_daemon` are normal deps of `mkrew-cli`, so the integration test may use them. Check the 2.4 sync listener trait path on docs.rs if `traits::Listener` does not resolve.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo build -j 4 -p mkrew-cli -p mkrew-daemon && cargo test -j 4 -p mkrew-cli --test cli hook_`
Expected: both new tests FAIL (no `hook.log`); `hook_outside_mkrew_exits_silently` passes.

- [ ] **Step 3: Write the failing unit tests** in `crates/mkrew-cli/src/hook.rs` `mod tests`:

```rust
    #[test]
    fn forward_target_ignores_relative_missing_and_itself() {
        let dir = tempfile::tempdir().unwrap();
        let me = std::env::current_exe().unwrap();
        assert_eq!(forward_target(Some("mk".into()), &me), None, "bare fallback");
        assert_eq!(forward_target(Some(dir.path().join("gone.exe").into()), &me), None);
        assert_eq!(forward_target(Some(me.clone().into()), &me), None, "itself");
        assert_eq!(forward_target(None, &me), None);
        let other = dir.path().join("mk.exe");
        std::fs::write(&other, b"").unwrap();
        assert_eq!(
            forward_target(Some(other.clone().into()), &me),
            Some(dunce::canonicalize(&other).unwrap())
        );
    }

    #[test]
    fn hook_log_rotates_above_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        log_failure(&logs, "omp", &anyhow::anyhow!("first"));
        let log = std::fs::read_to_string(logs.join("hook.log")).unwrap();
        assert!(log.ends_with(" hook omp: first\n"), "{log}");
        std::fs::write(logs.join("hook.log"), vec![b'x'; (HOOK_LOG_CAP + 1) as usize]).unwrap();
        log_failure(&logs, "claude", &anyhow::anyhow!("second"));
        assert_eq!(std::fs::metadata(logs.join("hook.log.1")).unwrap().len(), HOOK_LOG_CAP + 1);
        let log = std::fs::read_to_string(logs.join("hook.log")).unwrap();
        assert!(log.ends_with(" hook claude: second\n") && log.lines().count() == 1, "{log}");
    }
```

- [ ] **Step 4: Implement** in `crates/mkrew-cli/src/hook.rs`. Move today's body after the `MKREW_SESSION_ID` check into `fn report(id: &str, agent: String, args: &[String]) -> anyhow::Result<()>` (unchanged code from `let mkrew_session_id: Ulid = id.parse()…` through the `recv_timeout` match), then:

```rust
/// `hook.log` is rotated to `hook.log.1` above this size.
const HOOK_LOG_CAP: u64 = 1024 * 1024;

pub fn hook(agent: String, args: Vec<String>) -> anyhow::Result<ExitCode> {
    let Some(id) = std::env::var(ENV_SESSION_ID).ok().filter(|id| !id.is_empty()) else {
        return Ok(ExitCode::SUCCESS);
    };
    let me = std::env::current_exe().unwrap_or_default();
    if let Some(mk) = forward_target(std::env::var_os(ENV_MK), &me) {
        // The pane's daemon has its own `mk`; the hook config may name another one (ADR 0055).
        match std::process::Command::new(&mk).arg("hook").arg(&agent).args(&args).status() {
            Ok(status) => {
                let code = status.code().and_then(|c| u8::try_from(c).ok()).unwrap_or(1);
                return Ok(ExitCode::from(code));
            }
            Err(e) => log_to_data_dir(&agent, &anyhow::anyhow!("run {}: {e}", mk.display())),
        }
    }
    let result = report(&id, agent.clone(), &args);
    if let Err(e) = &result {
        log_to_data_dir(&agent, e);
    }
    result.map(|()| ExitCode::SUCCESS)
}

/// `MKREW_MK` when it is an existing file other than `me` (canonical paths, so no loop).
fn forward_target(mk: Option<OsString>, me: &Path) -> Option<PathBuf> {
    let mk = PathBuf::from(mk?);
    if !mk.is_absolute() {
        return None;
    }
    let target = dunce::canonicalize(&mk).ok()?;
    let me = dunce::canonicalize(me).ok();
    (Some(&target) != me.as_ref()).then_some(target)
}

fn log_to_data_dir(agent: &str, error: &anyhow::Error) {
    if let Ok(paths) = Paths::from_env() {
        log_failure(&paths.logs_dir(), agent, error);
    }
}

/// Appends one line to `<logs>/hook.log`: agents drop a hook's stderr, so this is the only
/// trace (ADR 0055). Every I/O error is ignored: logging must never fail the hook.
fn log_failure(logs: &Path, agent: &str, error: &anyhow::Error) {
    use std::io::Write as _;
    let _ = std::fs::create_dir_all(logs);
    let file = logs.join("hook.log");
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > HOOK_LOG_CAP) {
        let _ = std::fs::rename(&file, logs.join("hook.log.1"));
    }
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| dunce::canonicalize(p).ok())
        .map_or_else(|| "mk".to_owned(), |p| p.display().to_string());
    let error = format!("{error:#}").replace('\n', " ");
    let line = format!("{ms} {exe} mk {} hook {agent}: {error}\n", env!("CARGO_PKG_VERSION"));
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(line.as_bytes());
    }
}
```

Imports: `std::ffi::OsString`, `std::path::{Path, PathBuf}`, `mkrew_core::paths::ENV_MK`. Module doc: add "Inside a pane `MKREW_MK` names the daemon's `mk`; when that is another file, it runs instead. Failures go to `<data>/logs/hook.log`."

- [ ] **Step 5: Run the tests**

Run: `cargo test -j 4 -p mkrew-cli --bin mk hook && cargo build -j 4 -p mkrew-cli && cargo test -j 4 -p mkrew-cli --test cli hook_`
Expected: all pass, including `hook_outside_mkrew_exits_silently`.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-cli/src/hook.rs crates/mkrew-cli/tests/cli.rs
git commit -m "fix(cli): mk hook runs the daemon's mk and logs failures to hook.log"
```

### Task 3: Integrations stop running a bare `mk`

**Files:**
- Modify: `crates/mkrew-cli/assets/omp-extension.ts` (lines 10-25)
- Modify: `crates/mkrew-cli/src/integration.rs` (`CODEX_NOTIFY`, `claude_command`, `group_runs`, `claude_status`, `claude_edit`, `notify_is_ours`, `codex_status`, `codex_edit`, `owned_contents`, `run_on`, `print_status`, tests)
- Unchanged: `crates/mkrew-cli/assets/copilot-hooks.json` stays the template (`"exec": "mk"` replaced at install)

**Interfaces:**
- Consumes: `MKREW_MK` (Task 1) in the omp extension; `mk hook` forwarding (Task 2).
- Produces: `fn own_mk() -> anyhow::Result<String>`; `fn is_mk(program: &str) -> bool`; `fn claude_hook(mk: &str, event: &str) -> Value`; `fn claude_hook_is_ours(hook: &Value, event: &str) -> bool`; `claude_status(text: &str, mk: &str)`, `claude_edit(text: &str, install: bool, mk: &str)`, `codex_status(text: &str, mk: &str)`, `codex_edit(text: &str, install: bool, mk: &str)`, `owned_contents(agent: Agent, mk: &str) -> Result<String, Merged>`, `fn copilot_hooks(mk: &str) -> String`.

- [ ] **Step 1: Write the failing tests** in `integration.rs` `mod tests` (add `const MK: &str = r"C:\Program Files\Mkrew\bin\mk.exe";` at the top of the module):

```rust
    #[test]
    fn omp_extension_runs_the_daemons_mk() {
        assert!(OMP_EXTENSION.contains("process.env.MKREW_MK"), "{OMP_EXTENSION}");
        assert!(!OMP_EXTENSION.contains("execFile(\"mk\""), "no bare mk");
    }

    #[test]
    fn claude_install_replaces_an_old_bare_mk_hook() {
        let old = r#"{"hooks": {"Stop": [
  {"hooks": [{"type": "command", "command": "mk hook claude Stop"}]},
  {"hooks": [{"type": "command", "command": "mk-other"}]}
]}}"#;
        assert_eq!(claude_status(old, MK).unwrap(), Installed::Partly);
        let new = claude_edit(old, true, MK).unwrap().unwrap();
        let stop = json(&new)["hooks"]["Stop"].clone();
        assert_eq!(stop[0]["hooks"][0], json!({"type": "command", "command": "mk-other"}), "{new}");
        assert_eq!(
            stop[1]["hooks"][0],
            json!({"type": "command", "command": MK, "args": ["hook", "claude", "Stop"]}),
            "{new}"
        );
        assert!(!new.contains("mk hook claude"), "{new}");
        assert_eq!(claude_status(&new, MK).unwrap(), Installed::Yes);
        assert_eq!(claude_edit(&new, true, MK).unwrap(), None, "idempotent");
        // Uninstall drops ours in both forms and keeps the user's hook.
        let mixed = json!({"hooks": {"Stop": [
            {"hooks": [{"type": "command", "command": "mk hook claude Stop"}]},
            {"hooks": [claude_hook(r"D:\old\mk.exe", "Stop")]},
            {"hooks": [{"type": "command", "command": "mk-other"}]}
        ]}});
        let left = json(&claude_edit(&mixed.to_string(), false, MK).unwrap().unwrap());
        assert_eq!(left, json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mk-other"}]}]}}));
    }

    #[test]
    fn codex_install_upgrades_an_older_mkrew_notify() {
        let old = "notify = [\"mk\", \"hook\", \"codex\"]\n";
        assert_eq!(codex_status(old, MK).unwrap(), Installed::Partly);
        let new = codex_edit(old, true, MK).unwrap().unwrap();
        let doc: toml_edit::DocumentMut = new.parse().unwrap();
        let notify: Vec<&str> =
            doc["notify"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(notify, [MK, "hook", "codex"]);
        assert_eq!(codex_status(&new, MK).unwrap(), Installed::Yes);
        assert_eq!(codex_edit(&new, true, MK).unwrap(), None);
        assert_eq!(codex_edit(old, false, MK).unwrap().unwrap().trim(), "", "uninstall drops the old one");
    }
```

Update the existing tests to the new signatures (do not keep assertions on the old bare-`mk` text):
- `claude_install_merges_and_is_idempotent`, `claude_uninstall_removes_only_mkrew_hooks`, `claude_install_into_a_missing_file`: pass `MK`; expected entries are `claude_hook(MK, event)`.
- `codex_install_keeps_comments_and_stays_top_level`, `codex_never_replaces_another_notify_program`: pass `MK`; check the inserted `notify` by parsing (as above), not by text.
- `copilot_hooks_call_mk_for_the_events_mkrew_maps`: parse `copilot_hooks(MK)`; `assert_eq!(entries[0]["exec"], MK)`.
- `owned_files_install_replace_and_uninstall`: `ours` = `owned_contents(agent, MK).unwrap()`; add a case where the file holds `COPILOT_HOOKS` (the template, `"exec": "mk"`) → status `Partly`, install backs it up and writes `copilot_hooks(MK)`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -j 4 -p mkrew-cli --bin mk integration`
Expected: compile errors (new arities, missing fns).

- [ ] **Step 3: omp extension** — `crates/mkrew-cli/assets/omp-extension.ts`: replace the comment on line 12 and add the const above `function report`:

```ts
// The daemon's own `mk` (MKREW_MK, set in every Mkrew pane), so a stale `mk` on PATH never
// answers. One `mk hook omp <event> <json>` at a time, in order; `mk` logs its own failures
// to <data>/logs/hook.log, so errors are ignored here.
const mk = process.env.MKREW_MK || "mk";
```

and line 23:

```ts
        execFile(mk, ["hook", "omp", event, payload], { windowsHide: true, timeout: 10000 }, () => done());
```

- [ ] **Step 4: Claude, Codex, Copilot** in `integration.rs`:

```rust
/// The absolute path of this `mk`: hook configs name it, never a bare `mk` that `PATH` may
/// resolve to another version (ADR 0055).
fn own_mk() -> anyhow::Result<String> {
    let exe = std::env::current_exe().context("locate mk")?;
    let exe = dunce::canonicalize(&exe).with_context(|| format!("resolve {}", exe.display()))?;
    exe.into_os_string()
        .into_string()
        .map_err(|exe| anyhow::anyhow!("mk's path is not UTF-8: {exe:?}"))
}

/// Whether `program` is some `mk` (any folder, or bare): older Mkrew versions wrote `mk`.
fn is_mk(program: &str) -> bool {
    Path::new(program)
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("mk"))
}

/// Claude Code exec form: `command` runs directly with `args`, no shell (ADR 0055).
fn claude_hook(mk: &str, event: &str) -> Value {
    json!({"type": "command", "command": mk, "args": ["hook", "claude", event]})
}

/// Whether a hook entry is Mkrew's for `event`: the exec form with any `mk`, or the string
/// `mk hook claude <event>` older versions wrote.
fn claude_hook_is_ours(hook: &Value, event: &str) -> bool {
    let legacy = hook["command"] == format!("mk hook claude {event}").as_str();
    let exec = hook["args"] == json!(["hook", "claude", event])
        && hook["command"].as_str().is_some_and(is_mk);
    legacy || exec
}

/// Copilot's hooks file running `mk`; the asset is a template with `"exec": "mk"`.
fn copilot_hooks(mk: &str) -> String {
    COPILOT_HOOKS.replace(r#""exec": "mk""#, &format!(r#""exec": {}"#, json!(mk)))
}
```

- Delete `claude_command`. `group_runs(group: &Value, pred: impl Fn(&Value) -> bool) -> bool` → `hooks.iter().any(|hook| pred(hook))`.
- `claude_status(text, mk)`: per event, `current` = a group runs `|h| *h == claude_hook(mk, event)`; `ours` = a group runs `|h| claude_hook_is_ours(h, event)`. `Yes` when every event is current; `No` when no event has ours; else `Partly`.
- `claude_edit(text, install, mk)`: per event — `if install && groups.iter().any(|g| group_runs(g, |h| *h == claude_hook(mk, event))) { continue; }`; otherwise run the existing retain block with `list.retain(|hook| !claude_hook_is_ours(hook, event))`; then, if `install`, push `json!({"hooks": [claude_hook(mk, event)]})` and `continue`; keep the existing empty-group/event cleanup and the `after == before → None` return.
- Replace `CODEX_NOTIFY` by `const CODEX_ARGS: [&str; 2] = ["hook", "codex"];`. `notify_is_ours(doc)`: an array of exactly 3 strings, `is_mk(first)` and the rest equal `CODEX_ARGS`. New `notify_is_current(doc, mk)`: the array equals `[mk, "hook", "codex"]`. `codex_status(text, mk)`: current → `Yes`, ours → `Partly`, else `No`. `codex_edit(text, install, mk)`: `install && current → None`; `install && (ours || absent) → insert [mk, "hook", "codex"]` (toml_edit `Array::from_iter([mk, "hook", "codex"])`); `install && other → bail` (same message); `!install && ours → remove`; `!install && !ours → None`.
- `owned_contents(agent, mk) -> Result<String, Merged>`: `Omp → OMP_EXTENSION.to_owned()`, `Copilot → copilot_hooks(mk)`; `run_on` passes `&ours` to `run_owned`.
- `run_on`: `let mk = own_mk()?;` first; pass `&mk` to every function above. After a successful Install of Claude, Codex or Copilot print `note: hooks run {mk}; reinstall after moving Mkrew`.
- `print_status`: delete the "`mk` is not on PATH" warning (no config uses `PATH` now). If `mkrew_core::lookup::find`/`SEARCHED` lose their last user, check with `xd://lsp` references; delete only what has no other user.

- [ ] **Step 5: Run the tests**

Run: `cargo test -j 4 -p mkrew-cli --bin mk integration`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-cli/src/integration.rs crates/mkrew-cli/assets/omp-extension.ts
git commit -m "fix(cli): agent hooks run an absolute mk, never the one PATH finds first"
```

### Task 4: SPEC §6, ADR 0055, HUMAN_CHECKS, verify, smoke

**Files:**
- Modify: `docs/SPEC.md` §6 (lines ~464-482); §7 `hook.report` row (~538): change the agent list `(claude/codex/fake)` to `(claude/codex/omp/copilot/fake)` only if it still reads that way
- Create: `docs/decisions/0055-hooks-run-the-daemons-mk.md`
- Modify: `docs/HUMAN_CHECKS.md`

- [ ] **Step 1: SPEC §6** — replace the env bullet and the four agent bullets (Claude, Codex, Oh My Pi, Copilot) with:

```markdown
- The daemon sets `MKREW_SESSION_ID`, `MKREW_SOCKET` and `MKREW_MK` (the absolute path of the
  `mk` next to the daemon; bare `mk` when there is none) in every PTY it creates (ADR 0055).
- Hook configs never run a bare `mk`: `mk integration install` writes the absolute path of the
  `mk` that runs it; `mk hook` inside a pane re-runs itself through `MKREW_MK` when that is
  another file, so the daemon's own version always reports. Reinstall after moving Mkrew.
- Claude Code: hooks in `~/.claude/settings.json` for `SessionStart`, `UserPromptSubmit`,
  `Notification`, `Stop`, each in exec form `{"type": "command", "command": "<mk>", "args":
  ["hook", "claude", "<event>"]}` (no shell: Git Bash and PowerShell expand variables
  differently). The hook reads JSON from stdin and takes `session_id` as `native_session_id`.
- Codex: `notify = ["<mk>", "hook", "codex"]` in `~/.codex/config.toml` (argv, no shell); Codex
  passes JSON as an argument.
- Oh My Pi: `mk integration install omp` writes the Mkrew-owned extension
  `<PI_CODING_AGENT_DIR or ~/.omp/agent>/extensions/mkrew.ts`; it runs
  `$MKREW_MK hook omp <busy|needs_input|done|session_start> {"session_id": …}` (root session only:
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
```

Keep the "Owned files", "Install must", "check the CURRENT hook formats" and "`mk hook` without `MKREW_SESSION_ID`" bullets.

- [ ] **Step 2: ADR** — `docs/decisions/0055-hooks-run-the-daemons-mk.md`:

```markdown
# 0055 — Agent hooks run the daemon's own `mk`

- Date: 2026-10-14
- Status: accepted (changes SPEC §6 hook commands; no protocol change, `PROTOCOL_VERSION` 41)
- Milestone: none (bug fix, user report 2026-10-14)

## Context

Hooks ran a bare `mk`. On the user's machine `PATH` found mk 0.1.0 first while the daemon was
0.1.5: every `hello` got `version_mismatch`, the omp extension ignored the error, and from
2026-10-12 17:44 no omp session stored a `native_session_id`, so no pane resumed after a restart.

## Decision

- The daemon sets `MKREW_MK` (the `mk` next to its exe) in every PTY.
- omp's extension runs `MKREW_MK`. Claude Code (exec form), Codex (`notify` argv) and Copilot
  (`exec`) run the absolute path of the `mk` that installed the integration: Codex and Copilot
  `exec` run without a shell, and Claude's shell is Git Bash or PowerShell depending on the
  machine, so none of them can expand `MKREW_MK` portably.
- `mk hook` re-runs itself through `MKREW_MK` when that is another file: the installed path may
  belong to another install; the daemon's `mk` always speaks its protocol.
- A failed `mk hook` appends a line to `<data>/logs/hook.log`; the daemon warns on a refused
  `hello`.

## Alternatives

- Claude shell form `"${MKREW_MK:-mk}" hook claude …`: breaks where Claude falls back to
  PowerShell. Rejected.
- A daemon that accepts older protocol versions: far larger, and an old `mk` would still lack
  newer requests. Rejected.

## Consequences

- Integrations installed before this change show `partly installed`; run
  `"<Mkrew>\bin\mk.exe" integration install <agent>` once per agent.
- Moving the Mkrew folder needs a reinstall of the Claude/Codex/Copilot integrations.
- Claude Code must support exec-form hooks (`args`).
```

- [ ] **Step 3: HUMAN_CHECKS** — newest first under `## To verify (non-blocking)`:

```markdown
### To verify — 2026-10-14 Hooks run the daemon's own mk

- [ ] **Reinstall integrations first:** `& "$env:LOCALAPPDATA\Programs\Mkrew\bin\mk.exe" integration install omp`
  (and `claude`, `codex`, `copilot` if used). `mk integration status <agent>` → `installed`.
- [ ] **omp resumes:** open an omp pane, send one prompt → the session has a native id
  (`sqlite3 %APPDATA%\mkrew\mkrew.db "select native_session_id from session order by started_at desc limit 1"`
  is not empty). `mk daemon stop`, reopen Mkrew → the pane resumes the same conversation.
- [ ] **Stale mk on PATH:** with an old `mk` first on `PATH`, a new omp/Claude pane still gets a
  native id; `%APPDATA%\mkrew\logs\hook.log` gets no new line.
- [ ] **Claude Code exec form:** a Claude pane turns busy/idle with the new hooks (needs a Claude
  Code with exec-form hook support).
```

- [ ] **Step 4: Full verify**

Run: `sh scripts/verify.sh`
Expected: `verify: OK`.

- [ ] **Step 5: Smoke** (temp dirs only; no real agent config, no real daemon), PowerShell:

```powershell
$t = (New-Item -ItemType Directory "$env:TEMP\mkrew-hook-smoke-$(Get-Date -Format yyyyMMddHHmmss)").FullName
$mk = (Resolve-Path target\debug\mk.exe).Path
$env:CLAUDE_CONFIG_DIR="$t\claude"; $env:CODEX_HOME="$t\codex"; $env:COPILOT_HOME="$t\copilot"; $env:PI_CODING_AGENT_DIR="$t\omp"
foreach ($a in 'claude','codex','copilot','omp') { & $mk integration install $a; & $mk integration status $a }
Get-Content "$t\claude\settings.json", "$t\codex\config.toml", "$t\copilot\hooks\mkrew.json"
$env:MKREW_DATA_DIR="$t\data"; $env:MKREW_SOCKET="\\.\pipe\mkrew-hook-smoke"; $env:MKREW_SESSION_ID="01J00000000000000000000000"
& $mk hook omp busy '{}'; $LASTEXITCODE
Get-Content "$t\data\logs\hook.log"
'CLAUDE_CONFIG_DIR','CODEX_HOME','COPILOT_HOME','PI_CODING_AGENT_DIR','MKREW_DATA_DIR','MKREW_SOCKET','MKREW_SESSION_ID' | ForEach-Object { Remove-Item "Env:$_" }
```

Expected: every status `installed`; the configs name the absolute `mk` (exec form for Claude); `mk hook` exits 1; `hook.log` has one line `… mk <ver> hook omp: the daemon is not running …`.

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/0055-hooks-run-the-daemons-mk.md
git add -p docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "docs(spec): hooks run the daemon's own mk (ADR 0055)"
```

Final report: smoke output, the reinstall instruction, and that `PROTOCOL_VERSION` stayed 41 (no wire change).
