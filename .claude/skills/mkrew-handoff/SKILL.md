---
name: mkrew-handoff
description: Save state and end the session cleanly when the context window is getting long, a task is bigger than expected, or the session must stop mid-task. Use whenever you have read many large files, debugged for a long time, are about to compact, or the user says "wrap up", "handoff", or "save progress".
---

# Handoff

Goal: the next session (fresh context) can continue without re-discovering anything.

1. Get to a green state if at all possible: `bash scripts/verify.sh`. If the in-progress change
   cannot be made green quickly, keep only the green part (commit it) and describe the rest.
2. In `docs/PROGRESS.md`:
   - Keep the current task `[~]`, or split it: mark the finished part `[x]` and insert the
     remainder as new tasks right after it (`Tx.ya`, `Tx.yb`) with precise descriptions.
   - Rewrite "Notes for the next session" (≤ 8 lines): what is done, what is next, exact file
     paths/functions, traps discovered, commands that helped.
3. Commit: `wip(<scope>): <what is done>` (green) or `chore(progress): handoff <task>`.
4. Stop with a 3-line summary.
