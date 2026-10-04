---
name: mkrew-next
description: Run exactly one iteration of the Mkrew autonomous build loop — read docs/PROGRESS.md, pick the next task, implement it with tests, verify, review, update progress, commit, and stop. Use this whenever the user says "next", "continue", "keep building", "build the next task", "/mkrew-next", or when a headless build session starts in this repository, even if they do not name the skill.
---

# Mkrew: next task

Follow `docs/BUILD_LOOP.md` exactly. It is the single source of truth for the procedure; this
skill is only the entry point and a checklist.

## Checklist

1. Orient: `docs/PROGRESS.md`, `git status --short`, `git log --oneline -5`.
   Status not `RUNNING` → say why in one line, stop.
2. Milestone has no queue → use `mkrew-plan-milestone`, commit the plan, stop.
3. Pick one task (`[~]` first, else first `[ ]` with deps done). Mark `[~]`.
4. Load only the SPEC sections listed on the task. Delegate API lookups to `researcher`.
5. Implement + tests. UI work → `mkrew-ui-design` skill.
6. Verify with `mkrew-verify` (max 3 different fix attempts per failure, else mark `[!]`).
7. Review: `reviewer` subagent on the diff (+ `ui-reviewer` for UI). Fix BLOCKERs.
8. Update PROGRESS (task `[x]`, ticked criteria, notes ≤ 8 lines) in the same commit. Commit.
9. Queue empty → milestone wrap-up (BUILD_LOOP Step 8).
10. Stop with a 3-line summary. Do not start a second task.

Never ask the user a question during this skill; record assumptions in `docs/HUMAN_CHECKS.md`.
