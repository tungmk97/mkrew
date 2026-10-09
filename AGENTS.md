# Mkrew — agent instructions

Mkrew is a lightweight native Rust desktop app that orchestrates many coding agents
(Claude Code, Codex, Oh My Pi) across many projects. Daemon (PTYs, SQLite, state) + GPUI UI +
CLI `mk`, talking over a local socket. Zed is used for reading/editing code.

This repository is built **autonomously** by agents, one task per session.

## Start of every session

1. Read `docs/PROGRESS.md` — current milestone, task queue, notes from the last session.
2. Follow `docs/BUILD_LOOP.md` exactly: do ONE task, verify, review, record, commit, stop.
3. Open only the `docs/SPEC.md` sections (`§N`) your task lists. The spec is the source of truth.

## Golden rules

- One task per session. Then stop.
- Never ask the human mid-session. Decide using SPEC §2, record the question + your default in
  `docs/HUMAN_CHECKS.md`, continue.
- Never push, never rewrite history, never run destructive git commands, never touch files
  outside this repo.
- Every change ships with tests. No test may call a real agent or the network.
- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths.
- Look up crate APIs (gpui, portable-pty, alacritty_terminal, interprocess…) — never guess from memory.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same commit.
- UI: every color/spacing/radius/font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).

## Commands

| Purpose | Command |
| --- | --- |
| Full verify (fmt check, clippy -D warnings, tests) | `bash scripts/verify.sh` · Windows: `pwsh scripts/verify.ps1` |
| Format | `cargo fmt --all` |
| Fast test loop | `cargo test -p <crate> <filter>` |
| Snapshot review | `cargo insta review` (or accept: `cargo insta accept`) |
| Loop status | `bash scripts/status.sh` |

## Map

| Path | What |
| --- | --- |
| `docs/SPEC.md` | full spec, sections §1–§19 |
| `docs/PROGRESS.md` | loop state + task queue + notes (keep short) |
| `docs/BUILD_LOOP.md` | the per-session procedure |
| `docs/HUMAN_CHECKS.md` | everything waiting for the human |
| `docs/INBOX.md` | bugs/enhancements/features from the human; `mkrew-inbox` skill plans `[ ]` items |
| `docs/decisions/` | ADRs |
| `.claude/skills/` | skills (read by Claude Code and omp) |
| `.claude/agents/` | subagents: researcher, reviewer, ui-reviewer (Claude Code) |
| `.omp/agents/` | omp subagents: `final-reviewer` (`@slow`) for the superpowers final review; `plan-executor` (`@task`) runs executing-plans on Sonnet |
| `docs/MODEL_ROLES.md` | superpowers step → omp model role → model, and how to roll back |
| `crates/` | workspace crates (SPEC §4) |

## Conventions (summary of SPEC §17)

- `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- No `unwrap/expect` outside tests unless provably infallible (comment why).
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- Conventional Commits: `feat(daemon): …`, `fix(ui): …`, `test(core): …`, `chore(progress): …`.
