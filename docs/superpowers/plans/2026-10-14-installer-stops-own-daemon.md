# Installer stops only its own daemon — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install, upgrade and uninstall of the Mkrew install at `{app}` stop the per-user daemon only when that daemon's exe lives in `{app}\bin`.

**Architecture:** `mk daemon stop` gets an optional `--only-from <DIR>`. With it, `mk` asks the daemon for its pid (`daemon.status` already returns `pid`), reads that process's image path from the OS (`QueryFullProcessImageNameW` on Windows, `/proc/<pid>/exe` elsewhere) and stops the daemon only when the exe sits directly in `DIR`. Otherwise it prints `left running` and exits 0. `packaging/mkrew.iss` `StopDaemon` passes `--only-from "{app}\bin"`. When an older `mk` without the flag rejects it (clap usage error, exit code 2), it falls back to a plain `daemon stop`, so an in-place upgrade of a pre-fix install still frees the exe. No daemon change, no protocol change: `PROTOCOL_VERSION` stays 41.

**Tech Stack:** Rust (clap, dunce, windows-sys 0.61), Inno Setup 6 Pascal script.

**Spec:** the prompt below is the spec (2026-10-14 chat, "Prompt 1"). SPEC §8 ("Exactly one daemon per user") is unchanged.

<details><summary>Spec (verbatim)</summary>

```text
Fix: the Windows installer/uninstaller stops whichever Mkrew daemon is running, not the one of the install it is changing.

Evidence
- packaging/mkrew.iss `StopDaemon` (~line 120) runs `{app}\bin\mk.exe daemon stop`. It is called from
  `PrepareToInstall` and from `CurUninstallStepChanged(usUninstall)`.
- `mk daemon stop` talks to the default socket (\\.\pipe\mkrew-<user>), the single per-user daemon
  (SPEC 8 "Exactly one daemon per user"). So uninstalling a second copy (e.g. a test install in %TEMP%)
  stopped the user's real daemon that runs from %LOCALAPPDATA%\Programs\Mkrew. Reproduced 2026-10-14.

Goal
- Install/upgrade/uninstall of the install at {app} stops the daemon only if that daemon's exe lives in
  {app}\bin. An upgrade in place must still stop it (the exe has to be replaceable).
- A daemon running from another folder is left alone. If its exe is not in {app}, the uninstall
  must still finish.

Constraints
- Choose the smallest mechanism. Options: `daemon.status` already returns `pid`; compare that process's
  image path with {app}\bin, or add a guarded CLI flag. ... No new protocol version unless unavoidable.
- Packaging changes are approved for this task. Follow AGENTS.md (tests for any Rust change, no network,
  verify with `sh scripts/verify.sh`), Conventional Commit, commit only your own hunks.

Acceptance
- Rust part: a test showing the new check refuses a daemon whose exe is outside the given dir.
- Manual smoke (use .claude/skills/mkrew-build-setup; do NOT touch the real install in
  %LOCALAPPDATA%\Programs\Mkrew or its running daemon): with the real daemon running, install the setup
  to %TEMP%\mkrew-smoke-<timestamp>, uninstall it, then `mk session list` still answers (daemon alive).
  Note: uninstall of a smoke copy removes the shared AppId uninstall registry entry; report that,
  don't try to fix it here.
```
</details>

## Why this mechanism

| Option | Verdict |
|---|---|
| Add `exe` to `DaemonStatusResult` (daemon reports `current_exe()`) | Rejected: the user's running daemon (0.1.5) predates the field, so the smoke and every upgrade from 0.1.5 would see "unknown" — the exact case that must work. |
| `--only-from` + pid → image path in `mk` | Chosen: works against any daemon that speaks protocol 41, needs no daemon or protocol change. Cost: one `windows-sys` feature (`Win32_System_Threading`). |
| Check in Inno Pascal (WMI) | Rejected: no Rust test possible, more code. |

## Global Constraints

- `PROTOCOL_VERSION` stays `41`; no daemon change; no migration; no ADR (the spec is not changed, SPEC §8 still says one daemon per user).
- Workspace lint `unsafe_code = "deny"`: the Win32 call lives in a `#[cfg(windows)] #[allow(unsafe_code)] mod` with a `// SAFETY:` comment per `unsafe` block, like `crates/mkrew-cli/src/attach.rs` `windows_console`.
- No `unwrap`/`expect` outside tests.
- The only Cargo change is adding `"Win32_System_Threading"` to the `windows-sys` features of `crates/mkrew-cli/Cargo.toml`. `Cargo.lock` must not change (feature flags of an already-locked crate do not touch it; if it does change, stop and report). Stage only that hunk (`git add -p`): Cargo files often hold the user's WIP.
- Packaging change is approved for this task (spec). Stage only your hunks of `packaging/mkrew.iss`.
- Shell: in omp `bash` may be WSL. Run `cargo`/`git` directly; verify with `sh scripts/verify.sh` (Git Bash) or `pwsh scripts/verify.ps1`. Build with `-j 4`. C: is nearly full: check free space before the release build.
- Never stop, kill or uninstall the user's real daemon/install (`%LOCALAPPDATA%\Programs\Mkrew` and `%TEMP%\mkrew-setup-smoke2`). If an exe is locked, rename it to `*.locked-old.exe`.
- Conventional Commit, author `tungnv975@gmail.com`; explicit paths, never `git add -A`, never push.

## Review Focus

1. **Path spelled differently from the image path** (case, trailing `\`, 8.3 short names such as `C:\Users\NGUYEN~1\AppData\Local\Temp\…`): must still match. Both sides go through `dunce::canonicalize`. Pinned by `runs_from_ignores_case_and_trailing_separator` (Task 1) and the integration test, which passes the dir in upper case with a trailing separator.
2. **Directory that does not exist / exe not readable** (pid already gone, access denied): the daemon is left running and `mk` exits 0, so the uninstall finishes. Pinned by `runs_from_refuses_missing_paths` (Task 1); `process_exe` errors take the same "left running" branch (Task 2 code).
3. **Exe in a subfolder of `DIR`** (`{app}\bin\old\mkrew-daemon.exe`): not "in" `DIR`; refused. Pinned by `runs_from_refuses_an_exe_outside_the_dir` (Task 1).
4. **Upgrade of an install whose `mk` predates the flag**: the old `mk` exits 2 on the unknown flag; the script retries without it, so the exe is still freed. Checked in Task 3 smoke step 6 (reasoned; a real old-version upgrade is a HUMAN_CHECKS item).
5. **Quoting in Inno**: `"{app}\bin"` must not end with `\` before the closing quote (`\"` escapes the quote in Windows argv). `BinDir` has no trailing separator; checked in the Task 3 smoke (install dir with no spaces) and by reading the `Exec` line.

---

### Task 1: `runs_from` check

**Files:**
- Modify: `crates/mkrew-cli/src/daemon.rs` (new `pub fn runs_from`, tests module)

**Interfaces:**
- Produces: `pub fn runs_from(exe: &std::path::Path, dir: &std::path::Path) -> bool` in `crate::daemon` — true only when both paths canonicalize and `exe`'s parent equals `dir`.

- [ ] **Step 1: Write the failing tests** at the end of `crates/mkrew-cli/src/daemon.rs` (create `#[cfg(test)] mod tests` if the file has none; reuse it if it has):

```rust
#[cfg(test)]
mod tests {
    use super::runs_from;

    fn exe_in(dir: &std::path::Path) -> std::path::PathBuf {
        let exe = dir.join("mkrew-daemon.exe");
        std::fs::write(&exe, b"").unwrap();
        exe
    }

    #[test]
    fn runs_from_accepts_an_exe_directly_in_the_dir() {
        let bin = tempfile::tempdir().unwrap();
        assert!(runs_from(&exe_in(bin.path()), bin.path()));
    }

    #[test]
    fn runs_from_refuses_an_exe_outside_the_dir() {
        let (bin, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert!(!runs_from(&exe_in(other.path()), bin.path()), "another folder");
        let sub = bin.path().join("old");
        std::fs::create_dir(&sub).unwrap();
        assert!(!runs_from(&exe_in(&sub), bin.path()), "a subfolder is not the dir");
    }

    #[test]
    fn runs_from_refuses_missing_paths() {
        let bin = tempfile::tempdir().unwrap();
        let exe = exe_in(bin.path());
        assert!(!runs_from(&exe, &bin.path().join("gone")));
        assert!(!runs_from(&bin.path().join("gone.exe"), bin.path()));
    }

    #[test]
    fn runs_from_ignores_case_and_trailing_separator() {
        let bin = tempfile::tempdir().unwrap();
        let exe = exe_in(bin.path());
        let mut dir = bin.path().as_os_str().to_owned();
        dir.push(std::path::MAIN_SEPARATOR_STR);
        assert!(runs_from(&exe, std::path::Path::new(&dir)));
        if cfg!(windows) {
            let upper = bin.path().to_string_lossy().to_uppercase();
            assert!(runs_from(&exe, std::path::Path::new(&upper)));
        }
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -j 4 -p mkrew-cli --bin mk runs_from`
Expected: compile error `cannot find function runs_from`.

- [ ] **Step 3: Implement** in `crates/mkrew-cli/src/daemon.rs` (after `status`):

```rust
/// Whether `exe` lives directly in `dir`. Both are canonicalised, so case, `\\?\`, 8.3 short
/// names and a trailing separator do not matter; a path that cannot be resolved never matches.
pub fn runs_from(exe: &Path, dir: &Path) -> bool {
    match (dunce::canonicalize(exe), dunce::canonicalize(dir)) {
        (Ok(exe), Ok(dir)) => exe.parent() == Some(dir.as_path()),
        _ => false,
    }
}
```

Add `use std::path::Path;` if the file lacks it.

- [ ] **Step 4: Run the tests**

Run: `cargo test -j 4 -p mkrew-cli --bin mk runs_from`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-cli/src/daemon.rs
git commit -m "feat(cli): runs_from tells whether an exe lives in a folder"
```

### Task 2: `mk daemon stop --only-from <DIR>`

**Files:**
- Modify: `crates/mkrew-cli/Cargo.toml` (windows-sys feature `Win32_System_Threading`)
- Modify: `crates/mkrew-cli/src/daemon.rs` (`process_exe`, `stop` signature)
- Modify: `crates/mkrew-cli/src/main.rs:185-193` (`DaemonCommand::Stop`), `:273` (dispatch)
- Modify: any other caller of `daemon::stop` (find with `xd://lsp` references; e.g. `mk` upgrade/restart paths)
- Test: `crates/mkrew-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `daemon::runs_from` (Task 1); `DaemonStatusResult.pid: u32`.
- Produces: CLI `mk daemon stop [--only-from <DIR>]`. Output lines: `daemon not running` (exit 0), `daemon stopped (pid N)` (exit 0), new `daemon (pid N) runs from <exe>, not <DIR>; left running` (exit 0). Errors (connect failure, protocol mismatch) keep exit 1. `pub fn stop(paths: &Paths, only_from: Option<&Path>) -> anyhow::Result<ExitCode>`.

- [ ] **Step 1: Write the failing integration test** in `crates/mkrew-cli/tests/cli.rs`, next to `daemon_start_status_stop` (reuse its `Env`, `env.ok`, `env.mk`):

```rust
/// The installer's stop (`--only-from {app}\bin`) leaves a daemon of another folder alone and
/// stops one that runs from the folder (any spelling of it).
#[test]
fn daemon_stop_only_from_spares_a_daemon_of_another_folder() {
    let env = Env::new();
    env.ok(&["daemon", "start"]);
    let elsewhere = tempfile::tempdir().unwrap();
    let out = env.ok(&["daemon", "stop", "--only-from", &elsewhere.path().to_string_lossy()]);
    assert!(out.contains("left running"), "{out}");
    assert!(env.ok(&["daemon", "status"]).contains("daemon running"));

    // The test daemon runs from `target/<profile>/`, next to `mk`.
    let bin = std::path::Path::new(env!("CARGO_BIN_EXE_mk")).parent().unwrap();
    let mut spelled = bin.to_string_lossy().into_owned();
    if cfg!(windows) {
        spelled = spelled.to_uppercase();
    }
    spelled.push(std::path::MAIN_SEPARATOR);
    let out = env.ok(&["daemon", "stop", "--only-from", &spelled]);
    assert!(out.contains("daemon stopped"), "{out}");
    assert_eq!(env.mk(&["daemon", "status"]).status.code(), Some(1));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo build -j 4 -p mkrew-cli -p mkrew-daemon && cargo test -j 4 -p mkrew-cli --test cli daemon_stop_only_from`
Expected: FAIL — `mk` exits 2: `unexpected argument '--only-from'`.

- [ ] **Step 3: Add the Win32 feature** in `crates/mkrew-cli/Cargo.toml` line 46:

```toml
windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_System_Console", "Win32_System_Threading"] }
```

Update the comment above it: `# Get/SetConsoleMode: VT for mk attach; QueryFullProcessImageNameW: the daemon's exe (mk daemon stop --only-from).`

- [ ] **Step 4: Implement `process_exe`** in `crates/mkrew-cli/src/daemon.rs`. Confirm the exact 0.61 signatures on docs.rs (`windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_NAME_WIN32}`; `HANDLE` is a raw pointer in 0.61) before compiling:

```rust
/// The image path of process `pid`.
#[cfg(windows)]
fn process_exe(pid: u32) -> std::io::Result<std::path::PathBuf> {
    win::process_exe(pid)
}

/// The image path of process `pid` (Linux; elsewhere the call fails and the daemon is left).
#[cfg(not(windows))]
fn process_exe(pid: u32) -> std::io::Result<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    pub fn process_exe(pid: u32) -> io::Result<PathBuf> {
        // SAFETY: plain Win32 call; a null handle is checked below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0u16; 32_768];
        let mut len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // SAFETY: `handle` is open; `buf` holds `len` u16s and outlives the call.
        let ok = unsafe {
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        };
        let error = io::Error::last_os_error();
        // SAFETY: `handle` came from `OpenProcess` and is closed once.
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return Err(error);
        }
        Ok(OsString::from_wide(&buf[..len as usize]).into())
    }
}
```

- [ ] **Step 5: Gate `stop`** — replace the body start of `pub fn stop` in `daemon.rs`:

```rust
/// Stops the daemon. With `only_from`, only a daemon whose exe lives in that folder (the
/// installer passes `{app}\bin`): another install's daemon is left running (exit 0).
pub fn stop(paths: &Paths, only_from: Option<&Path>) -> anyhow::Result<ExitCode> {
    let Some(mut client) = try_connect(paths)? else {
        println!("daemon not running");
        return Ok(ExitCode::SUCCESS);
    };
    let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
    if let Some(dir) = only_from {
        let exe = process_exe(status.pid);
        if !exe.as_deref().is_ok_and(|exe| runs_from(exe, dir)) {
            let from = match &exe {
                Ok(exe) => exe.display().to_string(),
                Err(e) => format!("an unknown folder ({e})"),
            };
            println!(
                "daemon (pid {}) runs from {from}, not {}; left running",
                status.pid,
                dir.display()
            );
            return Ok(ExitCode::SUCCESS);
        }
    }
    client.call::<()>(Request::DaemonStop)?;
    // … rest unchanged (drop, wait for the lock, "daemon stopped (pid N)")
```

- [ ] **Step 6: CLI flag** in `crates/mkrew-cli/src/main.rs`:

```rust
    /// Stop the daemon and wait until it has exited.
    Stop {
        /// Only stop a daemon whose executable is in this folder (the installer passes its
        /// `bin`); a daemon of another install is left running.
        #[arg(long, value_name = "DIR")]
        only_from: Option<PathBuf>,
    },
```

Dispatch: `DaemonCommand::Stop { only_from } => daemon::stop(&paths, only_from.as_deref()),`. Update every other `daemon::stop(` caller to pass `None` (find them with `xd://lsp` references on `stop`).

- [ ] **Step 7: Run the tests**

Run: `cargo build -j 4 -p mkrew-cli -p mkrew-daemon && cargo test -j 4 -p mkrew-cli --test cli daemon_stop && cargo test -j 4 -p mkrew-cli --bin mk runs_from`
Expected: `daemon_start_status_stop` and `daemon_stop_only_from_spares_a_daemon_of_another_folder` pass; 4 `runs_from` tests pass. Also `git diff --stat Cargo.lock` is empty.

- [ ] **Step 8: Full verify**

Run: `sh scripts/verify.sh`
Expected: `verify: OK`.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-cli/src/daemon.rs crates/mkrew-cli/src/main.rs crates/mkrew-cli/tests/cli.rs
git add -p crates/mkrew-cli/Cargo.toml   # only the windows-sys feature hunk
git commit -m "feat(cli): mk daemon stop --only-from spares another install's daemon"
```

### Task 3: Installer passes `--only-from`, then smoke

**Files:**
- Modify: `packaging/mkrew.iss:118-148` (`StopDaemon`)
- Modify: `docs/HUMAN_CHECKS.md` (one entry, under `## To verify (non-blocking)`, newest first)

**Interfaces:**
- Consumes: `mk daemon stop --only-from <DIR>` (Task 2), exit code 2 = clap usage error (older `mk`).

- [ ] **Step 1: Change `StopDaemon`** in `packaging/mkrew.iss` (keep the callers in `PrepareToInstall` and `CurUninstallStepChanged` as they are):

```pascal
{ Asks the daemon to stop when it runs from this install's bin, so its exe can be replaced or
  removed; another install's daemon is left running (mk prints "left running", exit 0).
  Agent sessions resume on the next start (SPEC §5). An mk from before --only-from exits 2
  (unknown flag): an in-place upgrade still has to free the exe, so it stops the daemon the
  old way. }
procedure StopDaemon;
var
  Mk: String;
  Code: Integer;
begin
  Mk := ExpandConstant('{app}\bin\mk.exe');
  if not FileExists(Mk) then
    Exit;
  Exec(Mk, 'daemon stop --only-from "' + BinDir + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
  if Code = 2 then
    Exec(Mk, 'daemon stop', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;
```

`BinDir` (existing function, `ExpandConstant('{app}\bin')`) has no trailing backslash, so the closing quote is not escaped. If `BinDir` is declared below `StopDaemon` in the file, move `StopDaemon` below it (Pascal needs declaration before use).

- [ ] **Step 2: Read the skills** `.claude/skills/mkrew-build-setup/SKILL.md` and `memory://root/skills/mkrew-installer-smoke/SKILL.md` (template `scripts/smoke-steps.ps1`). Check free space on C: (`Get-PSDrive C`); stop and report if under 3 GB.

- [ ] **Step 3: Build the setup** (async, long timeout):

Run: `pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/package-setup.ps1`
Expected: `dist\mkrew-<ver>-x64-setup.exe`. ISCC error 110 → rerun with `-NoBuild`.

- [ ] **Step 4: Record the real daemon** (do not stop it):

Run: `mk daemon status` (the `mk` of the real install: `& "$env:LOCALAPPDATA\Programs\Mkrew\bin\mk.exe" daemon status`, or the one in `%TEMP%\mkrew-setup-smoke2\bin` if that is where it runs)
Expected: `daemon running`, note the pid. If no daemon runs, do not start one from the real install: report "no live daemon, smoke not possible" and stop here.

- [ ] **Step 5: Install to a fresh folder** (PowerShell):

```powershell
$d = "$env:TEMP\mkrew-smoke-$(Get-Date -Format yyyyMMdd-HHmmss)"
$setup = (Get-ChildItem dist\mkrew-*-x64-setup.exe | Sort-Object LastWriteTime | Select-Object -Last 1).FullName
Start-Process $setup -ArgumentList "/VERYSILENT","/CURRENTUSER","/SUPPRESSMSGBOXES","/NORESTART","/DIR=$d" -Wait -PassThru
& "$d\bin\mk.exe" --version
& "$d\bin\mk.exe" daemon stop --only-from "$d\bin"
```

Expected: install exit 0; the last command prints `daemon (pid <real pid>) runs from <real folder>\mkrew-daemon.exe, not …; left running`.

- [ ] **Step 6: Uninstall it and check the real daemon**

```powershell
Start-Process "$d\unins000.exe" -ArgumentList "/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART" -Wait -PassThru
& "<real install>\bin\mk.exe" daemon status
& "<real install>\bin\mk.exe" session list
```

Expected: uninstall exit 0, `$d\bin` gone, user PATH no longer has `$d\bin`; `daemon status` shows the SAME pid as Step 4; `session list` answers. Then check the registry: `Get-ChildItem HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall | Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -like 'Mkrew*' }` — expected: the real install's entry is gone (shared AppId). Report it; do not fix.

- [ ] **Step 7: HUMAN_CHECKS entry** in `docs/HUMAN_CHECKS.md`:

```markdown
### To verify — 2026-10-14 Installer stops only its own daemon

- [ ] **Upgrade in place:** with Mkrew running from `%LOCALAPPDATA%\Programs\Mkrew`, run the new
  setup over it → it finishes without "files in use"; reopen Mkrew → panes resume. (The first
  upgrade from 0.1.5 uses the old `mk`, which stops the daemon the old way.)
- [ ] **Uninstall of the smoke copy removed the real install's "Apps & features" entry** (shared
  AppId). Repair: rerun the real setup over `%LOCALAPPDATA%\Programs\Mkrew`.
```

- [ ] **Step 8: Commit**

```bash
git add -p packaging/mkrew.iss
git add -p docs/HUMAN_CHECKS.md
git commit -m "fix(packaging): stop the daemon only when it runs from this install"
```

Final report: smoke output of Steps 4–6 (pids before/after), and the registry-entry finding.
