---
name: mkrew-verify
description: Run and triage the Mkrew verification suite (cargo fmt check, clippy with -D warnings, all tests, insta snapshots) on Windows or Unix. Use before every commit, whenever tests or clippy fail, when snapshots change, or when the user says "verify", "run checks" or "make it green".
---

# Verify

## Run

- Unix / Git Bash: `bash scripts/verify.sh`
- Windows PowerShell: `pwsh scripts/verify.ps1`
- While iterating, run narrower checks first: `cargo test -p <crate> <filter>`, `cargo clippy -p <crate>`.

## Triage

- **fmt**: run `cargo fmt --all`, re-run verify.
- **clippy**: fix the code; `#[allow(...)]` only with a comment explaining why, never crate-wide.
- **test failure**: read the first failure only (`2>&1 | tail -n 60` or grep `panicked`). Do not
  paste huge logs into the conversation.
- **insta snapshot changed**: inspect the `.snap.new` diff. Accept (`cargo insta accept`) only if
  the new output is the intended behavior of the current task; otherwise fix the code.
- **flaky test** (timing, PTY): never just retry. Replace sleeps with `mk wait`-style state waits
  or explicit timeouts with generous limits; record the fix in notes.
- **Windows-only failure**: check `PATHEXT`, path separators, `\\?\` prefixes (`dunce`), CRLF,
  ConPTY quirks (output may include extra cursor/clear sequences).

## Attempts

At most 3 genuinely different fixes for the same failure. Then follow BUILD_LOOP Step 5
(mark `[!]` with a precise blocker note and stop).
