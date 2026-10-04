# Mkrew

A lightweight native Rust app to run and watch many coding agents (Claude Code, Codex, Oh My Pi)
across many projects. Spec: [`docs/SPEC.md`](docs/SPEC.md).

This repository is a **build kit**: it is set up so that Claude Code or omp can build the app
end to end, one task per session, with minimal human involvement.

## How the autonomous build works

```
scripts/autobuild.*  ──►  fresh agent session  ──►  docs/BUILD_LOOP.md
        ▲                  (clean context)            1. read docs/PROGRESS.md
        │                                             2. do ONE task + tests
        │                                             3. verify + subagent review
        └──── next iteration ◄── commit ◄──────────── 4. update PROGRESS, commit, stop
```

- **Memory lives in files, not in the chat:** `docs/PROGRESS.md` (state, task queue, notes for
  the next session) and git history. Every session starts with a clean context window, so quality
  does not degrade over a long build.
- **Context is injected automatically** (Claude Code): a SessionStart hook prints the loop state,
  next tasks and notes — also after `/compact`.
- **Subagents keep the main context small:** `researcher` (crate/API docs lookups), `reviewer`
  (diff review before commit), `ui-reviewer` (design system checks).
- **Skills:** `mkrew-next`, `mkrew-plan-milestone`, `mkrew-verify`, `mkrew-handoff`,
  `mkrew-ui-design` in `.claude/skills/` — read by both Claude Code and omp.
- **Humans only where needed:** the agent never stops to ask; it records questions with its
  chosen default in `docs/HUMAN_CHECKS.md`. The only blocking gate is the end of M0
  (UI framework decision after you look at the spike).

## Prerequisites (Windows)

1. Rust via [rustup](https://rustup.rs) (MSVC toolchain) and Visual Studio Build Tools with
   "Desktop development with C++" (includes the Windows SDK).
2. Git for Windows (provides Git Bash; Claude Code on Windows uses it too).
3. Claude Code (`claude`) and/or Oh My Pi (`omp`), logged in.
4. Optional: PowerShell 7 (`pwsh`), Zed.

## Start

```bash
# 1. Put this folder where you keep projects, then:
cd mkrew
git init && git add -A && git commit -m "chore: bootstrap mkrew build kit"

# 2. Recommended: watch the first task interactively to confirm permissions/hooks work
claude            # then type: /mkrew-next

# 3. Autonomous loop (Git Bash). Run it inside Herdr/tmux/Windows Terminal tab you keep open.
bash scripts/autobuild.sh --agent claude
#   or with omp:
bash scripts/autobuild.sh --agent omp
#   PowerShell alternative:
pwsh scripts/autobuild.ps1 -Agent claude
```

Useful options: `--until M2` (stop after a milestone), `--max-iter 20`, `--model <name>`,
`--timeout-min 90`. Extra CLI flags via `CLAUDE_FLAGS` / `OMP_FLAGS` environment variables.
For omp, check `omp --help` for the exact non-interactive/approval flags on your version
(default used: `-p --auto-approve`).

## Check in, answer, resume

```bash
bash scripts/status.sh          # status, current/next task, tags, open human checks
```

- Read `docs/HUMAN_CHECKS.md`. Answer items, move them to "Done".
- If the status is `WAITING_HUMAN`, edit `docs/PROGRESS.md` → `status: RUNNING`, commit, and
  start the loop again.
- Graceful stop: `touch .mkrew-build/STOP` (the running session finishes its task first).
- Logs of every session: `.mkrew-build/logs/`, loop summary: `.mkrew-build/loop.log`.

## Safety

- Headless sessions run with `acceptEdits` and the allow/deny list in `.claude/settings.json`:
  cargo, read-only git plus add/commit/tag/stash, basic file tools. `git push`, history rewrites,
  `git clean`, hard resets and `sudo` are denied. Commands outside the allow list are refused in
  headless mode and the agent works around them.
- The agent only edits this repository and never pushes. Review with `git log` whenever you like.
- Want zero prompts even for unusual commands? Only inside a VM/container:
  `CLAUDE_FLAGS="--dangerously-skip-permissions" bash scripts/autobuild.sh`.

## Repository map

| Path | Purpose |
| --- | --- |
| `AGENTS.md`, `CLAUDE.md` | agent instructions (omp reads AGENTS.md; Claude reads CLAUDE.md which imports it) |
| `docs/SPEC.md` | full product & technical spec (§1–§19) |
| `docs/BUILD_LOOP.md` | the procedure every session follows |
| `docs/PROGRESS.md` | loop state, milestone, task queue, notes |
| `docs/HUMAN_CHECKS.md` | everything waiting for you |
| `docs/GUIDE_VI.md` | step-by-step guide for the human (Vietnamese) |
| `docs/decisions/` | architecture decision records |
| `.claude/skills/` | build-loop skills |
| `.claude/agents/` | researcher, reviewer, ui-reviewer subagents |
| `.claude/settings.json` | permissions + SessionStart hook |
| `scripts/` | verify, autobuild, status, hooks |

---

## Hướng dẫn nhanh (Tiếng Việt)

1. Cài Rust (MSVC) + Visual Studio Build Tools, Git for Windows, Claude Code và/hoặc omp.
2. `git init`, commit lần đầu như trên.
3. Lần đầu nên chạy `claude` rồi gõ `/mkrew-next` để xem một task chạy trọn vẹn.
4. Sau đó chạy `bash scripts/autobuild.sh --agent claude` (hoặc `--agent omp`) trong một tab
   terminal để nguyên. Mỗi vòng là một phiên mới, ngữ cảnh sạch, làm đúng một task rồi commit.
5. Thỉnh thoảng chạy `bash scripts/status.sh` và đọc `docs/HUMAN_CHECKS.md`. Khi trạng thái là
   `WAITING_HUMAN` (sau M0), trả lời trong file đó, đổi `status: RUNNING` trong
   `docs/PROGRESS.md`, commit, rồi chạy lại vòng lặp.
6. Muốn dừng: tạo file `.mkrew-build/STOP`.
