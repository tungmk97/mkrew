# Mkrew autonomous build loop

This is the procedure every build session follows, whether it runs interactively
(`/mkrew-next` in Claude Code) or headless via `scripts/autobuild.*` (Claude Code or omp).

**Core idea: one session = one task.** Each session starts with a fresh context, reads the
small state file `docs/PROGRESS.md`, does exactly ONE task end to end, records what the next
session needs to know, commits, and stops. Long-term memory lives in git + `docs/PROGRESS.md`,
never in the conversation.

There is no human in the loop during a session. Never stop to ask a question. When a decision
is needed, pick the option most consistent with `docs/SPEC.md` §2 (principles), record the
question and your chosen default in `docs/HUMAN_CHECKS.md` → "Questions with assumed defaults",
and continue.

---

## Step 0 — Orient (keep it cheap)

1. Read `docs/PROGRESS.md` (the whole file; it is kept short on purpose).
2. Run `git status --short` and `git log --oneline -5`.
3. Read the `loop-state` block:
   - `status: WAITING_HUMAN`, `BLOCKED` or `DONE` → print one line explaining why and STOP.
   - `status: RUNNING` → continue.
4. Dirty working tree left by a crashed session:
   - If it clearly belongs to the task marked `[~]` (in progress), continue that task.
   - Otherwise run verify; if green commit it as `wip: recover uncommitted work` and note it;
     if red, `git stash push -m "recovered-<date>"` and note the stash name in PROGRESS.
   - Never discard work.

## Step 1 — Plan the milestone if needed

If the current milestone has no task queue (or the queue is empty but acceptance criteria are
unchecked), use the `mkrew-plan-milestone` skill. Planning IS the task for this session:
write the queue, commit `chore(progress): plan <milestone>`, STOP.

## Step 2 — Pick exactly one task

- Take the task marked `[~]` if any, else the first `[ ]` task whose dependencies are done.
- Mark it `[~]` in PROGRESS.md.
- Tasks marked `[!]` (blocked) are skipped unless their blocker note says it is now resolved.

## Step 3 — Load only the context the task needs

- Open only the SPEC sections listed on the task line (`spec: §6, §8`). Use search, not full reads.
- Find relevant code with grep/glob. Do not read whole crates "to get familiar".
- Crate APIs (gpui, portable-pty, alacritty_terminal, interprocess, …) change often: look them
  up instead of guessing. If subagents are available, delegate the lookup to the `researcher`
  subagent so docs do not flood your context; ask it for a short summary + minimal example.

## Step 4 — Implement

- Write the code and its tests together. Prefer small, focused changes (aim ≤ ~400 changed lines).
- Follow `docs/SPEC.md` §17 conventions. UI work: follow the `mkrew-ui-design` skill (§14).
- If the task turns out bigger than one session: finish a coherent, green slice, then split the
  rest into new tasks inserted right after this one (`T2.3a`, `T2.3b`, …).

## Step 5 — Verify

- Run `bash scripts/verify.sh` (Windows PowerShell: `pwsh scripts/verify.ps1`).
- Fix failures. Up to 3 genuinely different fix attempts for the same failure.
- Still failing after 3 attempts → revert to the last green state of this task if possible,
  mark the task `[!]` with a precise blocker note (error, what you tried, suspected cause),
  commit the note, and STOP. The next session will pick another task.

## Step 6 — Review

- If subagents are available: give the `reviewer` subagent the diff (`git diff`) and the task
  line. Fix every item it marks BLOCKER. Ignore pure style nits that clippy/fmt accept.
- UI changes: also run the `ui-reviewer` subagent against §14.
- No subagents (e.g. omp without task agents): self-review the diff against §2, §17 and the
  task's acceptance mapping before committing.

## Step 7 — Record and commit

Update `docs/PROGRESS.md` in the SAME commit as the code:

- Mark the task `[x]` and append ` — <short commit summary>`.
- Tick any milestone acceptance criteria now fully satisfied AND verified (automated test or
  explicit check). Criteria that need eyes or real agents → add to `docs/HUMAN_CHECKS.md`
  "To verify (non-blocking)" instead of ticking.
- Rewrite "Notes for the next session" (max 8 lines): gotchas, half-done threads, useful file
  paths. Delete notes that are no longer true.
- Move done tasks older than the last 10 to `docs/progress-archive.md` to keep PROGRESS short.

Commit with Conventional Commits, e.g. `feat(daemon): spawn sessions from TOML profiles`.
Never push. Never amend or rewrite earlier commits.

## Step 8 — Milestone wrap-up (only when the queue is empty)

1. Run verify once more.
2. Walk every acceptance criterion of the milestone in `docs/SPEC.md` §18. Each must be either
   ticked in PROGRESS (verified) or listed in HUMAN_CHECKS.
3. `git tag <milestone-lowercase>-done` (e.g. `m1-done`).
4. Set `milestone:` to the next one in SPEC §18 (M12 was dropped: M11 → M13) and replace the
   milestone section with an empty queue (the next session plans it).
5. Gates: after M0 set `status: WAITING_HUMAN` (UI framework decision). After M13 (the last
   milestone in SPEC §18) set `status: DONE`. Otherwise keep `RUNNING`.
6. Commit `chore(progress): complete <milestone>` and STOP.

## Step 9 — Stop

End the session with a 3-line summary: task done (or blocked), verify result, what is next.

---

## Context hygiene rules

- One task per session, always. Even if you have budget left, stop after Step 7/8.
- If you notice the conversation getting long (many large file reads, long debugging), stop
  early: get to a green state, use the `mkrew-handoff` skill, commit, STOP.
- Never paste large logs into the conversation; read the tail or grep for the error.
- Prefer `cargo test -p <crate> <filter>` while iterating; run the full verify only in Step 5.

## Safety rules

- Only modify files inside this repository.
- Never run `git push`, `git reset --hard`, `git clean`, `git checkout -- .`, or anything that
  deletes user data outside `target/`.
- Never install global tools without recording it in HUMAN_CHECKS (e.g. a missing system
  dependency): note it, choose a workaround if one exists, continue with other tasks.
