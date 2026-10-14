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
